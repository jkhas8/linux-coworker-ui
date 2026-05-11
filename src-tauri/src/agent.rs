// Drives a Claude Code conversation by spawning a fresh `claude` subprocess
// per user turn. We hand it `--resume <session_id>` from the second turn on,
// which keeps the conversation history while sidestepping the long-running
// stdin pipe that `claude --print` doesn't reliably support for multi-turn.

use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

#[derive(Debug, Clone, Deserialize)]
pub struct ImageAttachment {
    #[serde(rename = "mediaType")]
    pub media_type: String,
    pub data: String,
}

pub const EVENT_NAME: &str = "claude://event";

#[derive(Clone, serde::Serialize)]
pub struct AgentEvent {
    pub session_id: String,
    pub raw: Value,
}

/// Persists across turns. The actual `claude` process lives only for the
/// duration of a single user turn.
pub struct AgentSession {
    pub local_id: String, // app-side id (uuid), used as the session_id in events
    working_dir: PathBuf,
    mcp_config: PathBuf,
    permission_mode: String,
    /// Claude Code's session id, learned from the `system/init` event after
    /// the first turn — used with `--resume` from turn 2 onwards.
    claude_session_id: Arc<Mutex<Option<String>>>,
    /// Currently running subprocess for the in-flight turn, if any.
    current: Arc<Mutex<Option<Child>>>,
    app: AppHandle,
}

impl AgentSession {
    pub fn new(
        app: AppHandle,
        working_dir: PathBuf,
        mcp_config: PathBuf,
        permission_mode: &str,
    ) -> Self {
        Self {
            local_id: uuid::Uuid::new_v4().to_string(),
            working_dir,
            mcp_config,
            permission_mode: permission_mode.to_string(),
            claude_session_id: Arc::new(Mutex::new(None)),
            current: Arc::new(Mutex::new(None)),
            app,
        }
    }

    pub async fn send_user_message(&self, text: &str, images: &[ImageAttachment]) -> Result<()> {
        let payload = build_user_message(text, images);
        if payload.is_none() {
            return Ok(()); // nothing to send
        }
        let line = format!("{}\n", serde_json::to_string(&payload.unwrap())?);

        // Build argv. Reuse the existing claude session id if we have one.
        let resume = self.claude_session_id.lock().await.clone();

        let mut cmd = Command::new("claude");
        cmd.current_dir(&self.working_dir)
            .args([
                "--print",
                "--input-format",
                "stream-json",
                "--output-format",
                "stream-json",
                "--verbose",
                "--strict-mcp-config",
                "--mcp-config",
            ])
            .arg(&self.mcp_config)
            .arg("--permission-mode")
            .arg(&self.permission_mode);

        if let Some(sid) = resume.as_deref() {
            cmd.arg("--resume").arg(sid);
        }

        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = cmd
            .spawn()
            .context("failed to spawn `claude`; is it on PATH?")?;

        let stdout = child.stdout.take().context("missing child stdout")?;
        let stderr = child.stderr.take().context("missing child stderr")?;
        let mut stdin = child.stdin.take().context("missing child stdin")?;

        // Feed the single user turn, then close stdin so claude knows EOF
        // and exits when the turn completes.
        stdin.write_all(line.as_bytes()).await?;
        stdin.flush().await?;
        drop(stdin);

        // Stash the child so cancel/end_session can kill it mid-turn.
        {
            let mut cur = self.current.lock().await;
            *cur = Some(child);
        }

        spawn_reader(
            self.app.clone(),
            self.local_id.clone(),
            self.claude_session_id.clone(),
            self.current.clone(),
            stdout,
            false,
        );
        spawn_reader(
            self.app.clone(),
            self.local_id.clone(),
            self.claude_session_id.clone(),
            self.current.clone(),
            stderr,
            true,
        );

        Ok(())
    }

    /// Kill any in-flight turn and forget the claude session id (so the next
    /// message starts a fresh conversation).
    pub async fn end(&self) -> Result<()> {
        {
            let mut cur = self.current.lock().await;
            if let Some(mut c) = cur.take() {
                let _ = c.start_kill();
                let _ = c.wait().await;
            }
        }
        let mut sid = self.claude_session_id.lock().await;
        *sid = None;
        Ok(())
    }
}

fn build_user_message(text: &str, images: &[ImageAttachment]) -> Option<Value> {
    let mut content: Vec<Value> = Vec::with_capacity(images.len() + 1);
    for img in images {
        content.push(json!({
            "type": "image",
            "source": {
                "type": "base64",
                "media_type": img.media_type,
                "data": img.data,
            }
        }));
    }
    if !text.is_empty() {
        content.push(json!({ "type": "text", "text": text }));
    }
    if content.is_empty() {
        return None;
    }
    Some(json!({
        "type": "user",
        "message": { "role": "user", "content": content }
    }))
}

fn spawn_reader<R>(
    app: AppHandle,
    local_id: String,
    claude_sid: Arc<Mutex<Option<String>>>,
    current: Arc<Mutex<Option<Child>>>,
    reader: R,
    is_stderr: bool,
) where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let mut lines = BufReader::new(reader).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if line.is_empty() {
                continue;
            }
            let raw: Value = if is_stderr {
                json!({ "type": "stderr", "text": line })
            } else {
                serde_json::from_str(&line)
                    .unwrap_or_else(|_| json!({ "type": "unparsed", "text": line }))
            };

            // Latch claude's session_id when we see the init event so the
            // next turn can --resume it.
            if !is_stderr
                && raw.get("type").and_then(Value::as_str) == Some("system")
                && raw.get("subtype").and_then(Value::as_str) == Some("init")
            {
                if let Some(sid) = raw.get("session_id").and_then(Value::as_str) {
                    let mut g = claude_sid.lock().await;
                    if g.is_none() {
                        *g = Some(sid.to_string());
                        tracing::info!("captured claude session_id: {sid}");
                    }
                }
            }

            // When the turn completes, clear the current child slot.
            if !is_stderr && raw.get("type").and_then(Value::as_str) == Some("result") {
                let mut cur = current.lock().await;
                if let Some(mut c) = cur.take() {
                    let _ = c.wait().await;
                }
            }

            let _ = app.emit(
                EVENT_NAME,
                AgentEvent {
                    session_id: local_id.clone(),
                    raw,
                },
            );
        }
    });
}
