// Spawns and supervises a `claude` subprocess in stream-json mode.
//
// Protocol: each line on the child's stdout/stdin is a JSON object. We forward
// every output event to the frontend as a `claude://event` Tauri event so the
// UI can decide how to render assistant text, tool calls, results, etc.

use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::Mutex;

pub const EVENT_NAME: &str = "claude://event";

#[derive(Clone, serde::Serialize)]
pub struct AgentEvent {
    pub session_id: String,
    pub raw: Value,
}

pub struct AgentSession {
    pub id: String,
    child: Child,
    stdin: Arc<Mutex<ChildStdin>>,
}

impl AgentSession {
    pub async fn start(
        app: AppHandle,
        working_dir: PathBuf,
        mcp_config: PathBuf,
        permission_mode: &str,
    ) -> Result<Self> {
        let id = uuid::Uuid::new_v4().to_string();

        let mut child = Command::new("claude")
            .current_dir(&working_dir)
            .args([
                "--print",
                "--input-format",
                "stream-json",
                "--output-format",
                "stream-json",
                "--verbose",
                "--strict-mcp-config",
                "--mcp-config",
                mcp_config.to_string_lossy().as_ref(),
                "--permission-mode",
                permission_mode,
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("failed to spawn `claude`; is it on PATH?")?;

        let stdout = child.stdout.take().context("missing child stdout")?;
        let stderr = child.stderr.take().context("missing child stderr")?;
        let stdin = child.stdin.take().context("missing child stdin")?;
        let stdin = Arc::new(Mutex::new(stdin));

        spawn_reader(app.clone(), id.clone(), stdout, false);
        spawn_reader(app.clone(), id.clone(), stderr, true);

        Ok(Self { id, child, stdin })
    }

    pub async fn send_user_text(&self, text: &str) -> Result<()> {
        let msg = json!({
            "type": "user",
            "message": {
                "role": "user",
                "content": [{ "type": "text", "text": text }]
            }
        });
        let line = format!("{}\n", serde_json::to_string(&msg)?);
        let mut s = self.stdin.lock().await;
        s.write_all(line.as_bytes()).await?;
        s.flush().await?;
        Ok(())
    }

    pub async fn shutdown(mut self) -> Result<()> {
        // Closing stdin signals end-of-input to claude --print.
        {
            let mut s = self.stdin.lock().await;
            s.shutdown().await.ok();
        }
        let _ = self.child.wait().await;
        Ok(())
    }
}

fn spawn_reader<R>(app: AppHandle, session_id: String, reader: R, is_stderr: bool)
where
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
            let _ = app.emit(
                EVENT_NAME,
                AgentEvent {
                    session_id: session_id.clone(),
                    raw,
                },
            );
        }
    });
}
