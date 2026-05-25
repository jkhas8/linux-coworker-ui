// Drives a Claude Code conversation by spawning a fresh `claude` subprocess
// per user turn. We hand it `--resume <session_id>` from the second turn on,
// which keeps the conversation history while sidestepping the long-running
// stdin pipe that `claude --print` doesn't reliably support for multi-turn.

use crate::storage::ConversationLog;
use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
#[allow(dead_code)]
pub enum UserAttachment {
    Image {
        #[serde(rename = "mediaType")]
        media_type: String,
        data: String,
        #[serde(default)]
        name: Option<String>,
    },
    Pdf {
        #[serde(rename = "mediaType", default)]
        media_type: Option<String>,
        data: String,
        #[serde(default)]
        name: Option<String>,
    },
    Text {
        #[serde(rename = "mediaType", default)]
        media_type: Option<String>,
        text: String,
        #[serde(default)]
        name: Option<String>,
    },
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
    /// Optional sink for persisting every stream event to disk. When None,
    /// the session runs purely in-memory (legacy behaviour, useful in tests
    /// and when no workspace is active).
    conversation_log: Option<ConversationLog>,
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
            conversation_log: None,
            app,
        }
    }

    /// Attach a conversation log: every non-stderr event the session
    /// emits will be appended to it as a JSON line.
    pub fn with_conversation_log(mut self, log: ConversationLog) -> Self {
        self.conversation_log = Some(log);
        self
    }

    /// Seed the Claude Code session id this AgentSession should resume
    /// from. Used by the reopen flow (Story 10) — when present, the very
    /// first `send_user_message` spawns `claude` with
    /// `--resume <session_id>` instead of starting a fresh session.
    ///
    /// Construction is single-owner so `try_lock` is safe here.
    pub fn with_resume_session_id(self, session_id: String) -> Self {
        if let Ok(mut g) = self.claude_session_id.try_lock() {
            *g = Some(session_id);
        } else {
            tracing::warn!("with_resume_session_id: session lock contended; resume not seeded");
        }
        self
    }

    pub async fn send_user_message(
        &self,
        text: &str,
        attachments: &[UserAttachment],
    ) -> Result<()> {
        let payload = build_user_message(text, attachments);
        if payload.is_none() {
            return Ok(()); // nothing to send
        }
        let line = format!("{}\n", serde_json::to_string(&payload.unwrap())?);

        // Refuse to spawn if the workspace's folder has disappeared since
        // the workspace was created (user moved/deleted it). A friendly
        // error is much better than the cryptic ENOENT claude would emit.
        if !self.working_dir.is_dir() {
            anyhow::bail!("workspace folder no longer exists: {:?}", self.working_dir);
        }

        // Build argv. Reuse the existing claude session id if we have one.
        let resume = self.claude_session_id.lock().await.clone();
        let used_resume = resume.is_some();
        // Tracks whether the stream emitted a system/init event before
        // claude exited. If we asked for --resume and the process dies
        // without ever sending init, we infer a resume failure and emit
        // a synthetic event so the frontend can surface a recovery banner.
        let saw_init = Arc::new(AtomicBool::new(false));

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
            self.conversation_log.clone(),
            stdout,
            false,
            used_resume,
            saw_init.clone(),
        );
        spawn_reader(
            self.app.clone(),
            self.local_id.clone(),
            self.claude_session_id.clone(),
            self.current.clone(),
            self.conversation_log.clone(),
            stderr,
            true,
            used_resume,
            saw_init,
        );

        Ok(())
    }

    /// Kill any in-flight turn but **keep** the claude session id so the user
    /// can continue the same conversation. Used by the in-composer Stop button.
    pub async fn cancel_turn(&self) -> Result<()> {
        let mut cur = self.current.lock().await;
        if let Some(mut c) = cur.take() {
            let _ = c.start_kill();
            let _ = c.wait().await;
        }
        Ok(())
    }

    /// Kill any in-flight turn and forget the claude session id (so the next
    /// message starts a fresh conversation). Used by the "+ New" button.
    pub async fn end(&self) -> Result<()> {
        self.cancel_turn().await?;
        let mut sid = self.claude_session_id.lock().await;
        *sid = None;
        Ok(())
    }
}

fn build_user_message(text: &str, attachments: &[UserAttachment]) -> Option<Value> {
    let mut content: Vec<Value> = Vec::with_capacity(attachments.len() + 1);
    for att in attachments {
        match att {
            UserAttachment::Image {
                media_type, data, ..
            } => {
                content.push(json!({
                    "type": "image",
                    "source": {
                        "type": "base64",
                        "media_type": media_type,
                        "data": data,
                    }
                }));
            }
            UserAttachment::Pdf {
                media_type,
                data,
                name,
            } => {
                let mut doc = json!({
                    "type": "document",
                    "source": {
                        "type": "base64",
                        "media_type": media_type.as_deref().unwrap_or("application/pdf"),
                        "data": data,
                    }
                });
                if let Some(n) = name {
                    doc["title"] = json!(n);
                }
                content.push(doc);
            }
            UserAttachment::Text {
                text: body, name, ..
            } => {
                // Inline the file's contents as a fenced text block so the model
                // sees it as part of the user's turn. Header gives it a filename.
                let label = name.as_deref().unwrap_or("file");
                let inlined = format!("=== {label} ===\n{body}");
                content.push(json!({ "type": "text", "text": inlined }));
            }
        }
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

#[allow(clippy::too_many_arguments)]
fn spawn_reader<R>(
    app: AppHandle,
    local_id: String,
    claude_sid: Arc<Mutex<Option<String>>>,
    current: Arc<Mutex<Option<Child>>>,
    conversation_log: Option<ConversationLog>,
    reader: R,
    is_stderr: bool,
    used_resume: bool,
    saw_init: Arc<AtomicBool>,
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
                saw_init.store(true, Ordering::SeqCst);
                if let Some(sid) = raw.get("session_id").and_then(Value::as_str) {
                    let mut g = claude_sid.lock().await;
                    if g.is_none() {
                        *g = Some(sid.to_string());
                        tracing::info!("captured claude session_id: {sid}");
                    }
                }
            }

            // Persist this event to the conversation log if one is
            // attached. Skip stderr lines — those are framework noise that
            // belongs in the UI's error rail, not in replayable history.
            if !is_stderr {
                if let Some(log) = conversation_log.as_ref() {
                    if let Err(err) = log.append_event(&raw) {
                        tracing::warn!(?err, "failed to append event to conversation log");
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

        // EOF on this stream. The stdout reader is the canonical place to
        // observe the process exiting cleanly. If we asked claude to
        // --resume an existing session AND it died before ever emitting
        // `system/init`, that's a resume failure: surface it so the
        // frontend can show a "Continue fresh" banner.
        if !is_stderr && used_resume && !saw_init.load(Ordering::SeqCst) {
            let mut cur = current.lock().await;
            if let Some(mut c) = cur.take() {
                let exit = c.wait().await;
                let code = exit.ok().and_then(|s| s.code());
                let _ = app.emit(
                    EVENT_NAME,
                    AgentEvent {
                        session_id: local_id.clone(),
                        raw: json!({
                            "type": "resume_failure",
                            "exit_code": code,
                        }),
                    },
                );
            }
        }
    });
}
