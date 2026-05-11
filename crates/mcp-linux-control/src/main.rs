// MCP server (stdio JSON-RPC 2.0) exposing Linux desktop control tools.
// Speaks MCP 2024-11-05.

use anyhow::{bail, Context, Result};
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;

const PROTOCOL_VERSION: &str = "2024-11-05";
const SERVER_NAME: &str = "mcp-linux-control";
const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Deserialize)]
struct RpcRequest {
    jsonrpc: String,
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Debug, Serialize)]
struct RpcResponse {
    jsonrpc: &'static str,
    id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<RpcError>,
}

#[derive(Debug, Serialize)]
struct RpcError {
    code: i32,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();
    let mut lines = BufReader::new(stdin).lines();

    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }
        let req: RpcRequest = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("invalid jsonrpc line: {e}: {line}");
                continue;
            }
        };
        if req.jsonrpc != "2.0" {
            tracing::warn!("non-2.0 jsonrpc: {}", req.jsonrpc);
            continue;
        }

        // Notifications have no id and expect no response.
        let is_notification = req.id.is_none();
        let id = req.id.clone().unwrap_or(Value::Null);

        let result = handle(&req.method, req.params).await;

        if is_notification {
            continue;
        }

        let resp = match result {
            Ok(v) => RpcResponse {
                jsonrpc: "2.0",
                id,
                result: Some(v),
                error: None,
            },
            Err(e) => RpcResponse {
                jsonrpc: "2.0",
                id,
                result: None,
                error: Some(RpcError {
                    code: -32000,
                    message: e.to_string(),
                    data: None,
                }),
            },
        };

        let s = serde_json::to_string(&resp)?;
        stdout.write_all(s.as_bytes()).await?;
        stdout.write_all(b"\n").await?;
        stdout.flush().await?;
    }
    Ok(())
}

async fn handle(method: &str, params: Value) -> Result<Value> {
    match method {
        "initialize" => Ok(json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": { "tools": {} },
            "serverInfo": { "name": SERVER_NAME, "version": SERVER_VERSION },
        })),
        "notifications/initialized" | "initialized" => Ok(Value::Null),
        "tools/list" => Ok(json!({ "tools": tool_definitions() })),
        "tools/call" => {
            let name = params
                .get("name")
                .and_then(Value::as_str)
                .context("tools/call missing name")?;
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            call_tool(name, args).await
        }
        "ping" => Ok(json!({})),
        other => bail!("unknown method: {other}"),
    }
}

fn tool_definitions() -> Value {
    json!([
        {
            "name": "screenshot",
            "description": "Capture the current X11 screen and return it as an image. Use this to see what is on the user's screen before deciding where to click.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "region": {
                        "type": "string",
                        "description": "Optional region in `WxH+X+Y` form. Omit to capture the full screen."
                    }
                }
            }
        },
        {
            "name": "xdo_click",
            "description": "Click at the current cursor location or at given coordinates. Destructive: prefer screenshot first.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "button": { "type": "integer", "enum": [1, 2, 3], "default": 1, "description": "1=left, 2=middle, 3=right" },
                    "x": { "type": "integer" },
                    "y": { "type": "integer" },
                    "double": { "type": "boolean", "default": false }
                }
            }
        },
        {
            "name": "xdo_move",
            "description": "Move the mouse cursor to absolute screen coordinates.",
            "inputSchema": {
                "type": "object",
                "required": ["x", "y"],
                "properties": {
                    "x": { "type": "integer" },
                    "y": { "type": "integer" }
                }
            }
        },
        {
            "name": "xdo_type",
            "description": "Type a literal string into the focused window.",
            "inputSchema": {
                "type": "object",
                "required": ["text"],
                "properties": {
                    "text": { "type": "string" },
                    "delay_ms": { "type": "integer", "default": 12, "description": "Per-character delay in ms" }
                }
            }
        },
        {
            "name": "xdo_key",
            "description": "Send a keystroke or chord, e.g. `Return`, `ctrl+s`, `alt+Tab`.",
            "inputSchema": {
                "type": "object",
                "required": ["key"],
                "properties": { "key": { "type": "string" } }
            }
        },
        {
            "name": "launch_app",
            "description": "Launch a desktop application detached from this process. Returns immediately.",
            "inputSchema": {
                "type": "object",
                "required": ["command"],
                "properties": {
                    "command": { "type": "string", "description": "Argv, e.g. `firefox --new-window https://example.com`" }
                }
            }
        },
        {
            "name": "list_windows",
            "description": "List currently open X11 windows with their titles and IDs (uses wmctrl).",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "focus_window",
            "description": "Bring a window to the foreground by title substring (uses wmctrl -a).",
            "inputSchema": {
                "type": "object",
                "required": ["title"],
                "properties": { "title": { "type": "string" } }
            }
        }
    ])
}

async fn call_tool(name: &str, args: Value) -> Result<Value> {
    let content = match name {
        "screenshot" => screenshot(args).await?,
        "xdo_click" => xdo_click(args).await?,
        "xdo_move" => xdo_move(args).await?,
        "xdo_type" => xdo_type(args).await?,
        "xdo_key" => xdo_key(args).await?,
        "launch_app" => launch_app(args).await?,
        "list_windows" => list_windows().await?,
        "focus_window" => focus_window(args).await?,
        other => bail!("unknown tool: {other}"),
    };
    Ok(json!({ "content": content, "isError": false }))
}

fn require_bin(name: &str) -> Result<()> {
    which::which(name).with_context(|| {
        format!("`{name}` not found on PATH. Install it (e.g. `sudo apt install {name}`).")
    })?;
    Ok(())
}

fn text(s: impl Into<String>) -> Value {
    json!([{ "type": "text", "text": s.into() }])
}

async fn run_ok(cmd: &str, args: &[&str]) -> Result<String> {
    let out = Command::new(cmd).args(args).output().await?;
    if !out.status.success() {
        bail!(
            "{cmd} {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr)
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

async fn screenshot(args: Value) -> Result<Value> {
    let region = args.get("region").and_then(Value::as_str);
    let tmp = tempfile_path("png");

    if which::which("maim").is_ok() {
        let mut a: Vec<String> = vec![];
        if let Some(r) = region {
            a.push("-g".into());
            a.push(r.into());
        }
        a.push(tmp.clone());
        let refs: Vec<&str> = a.iter().map(String::as_str).collect();
        run_ok("maim", &refs).await?;
    } else if which::which("scrot").is_ok() {
        let mut a: Vec<String> = vec![];
        if let Some(r) = region {
            a.push("-a".into());
            a.push(r.into());
        }
        a.push(tmp.clone());
        let refs: Vec<&str> = a.iter().map(String::as_str).collect();
        run_ok("scrot", &refs).await?;
    } else {
        bail!("install `maim` or `scrot` for screenshots (`sudo apt install maim`)");
    }

    let bytes = tokio::fs::read(&tmp).await?;
    let _ = tokio::fs::remove_file(&tmp).await;
    let b64 = B64.encode(&bytes);
    Ok(json!([{
        "type": "image",
        "data": b64,
        "mimeType": "image/png"
    }]))
}

async fn xdo_click(args: Value) -> Result<Value> {
    require_bin("xdotool")?;
    let button = args.get("button").and_then(Value::as_i64).unwrap_or(1);
    let double = args.get("double").and_then(Value::as_bool).unwrap_or(false);

    if let (Some(x), Some(y)) = (
        args.get("x").and_then(Value::as_i64),
        args.get("y").and_then(Value::as_i64),
    ) {
        run_ok(
            "xdotool",
            &["mousemove", "--sync", &x.to_string(), &y.to_string()],
        )
        .await?;
    }

    let mut argv = vec!["click"];
    if double {
        argv.push("--repeat");
        argv.push("2");
    }
    let b = button.to_string();
    argv.push(&b);
    run_ok("xdotool", &argv).await?;
    Ok(text(format!("clicked button {button}")))
}

async fn xdo_move(args: Value) -> Result<Value> {
    require_bin("xdotool")?;
    let x = args.get("x").and_then(Value::as_i64).context("missing x")?;
    let y = args.get("y").and_then(Value::as_i64).context("missing y")?;
    run_ok(
        "xdotool",
        &["mousemove", "--sync", &x.to_string(), &y.to_string()],
    )
    .await?;
    Ok(text(format!("moved to {x},{y}")))
}

async fn xdo_type(args: Value) -> Result<Value> {
    require_bin("xdotool")?;
    let text_arg = args
        .get("text")
        .and_then(Value::as_str)
        .context("missing text")?;
    let delay = args
        .get("delay_ms")
        .and_then(Value::as_i64)
        .unwrap_or(12)
        .to_string();
    run_ok("xdotool", &["type", "--delay", &delay, "--", text_arg]).await?;
    Ok(text(format!("typed {} chars", text_arg.chars().count())))
}

async fn xdo_key(args: Value) -> Result<Value> {
    require_bin("xdotool")?;
    let key = args
        .get("key")
        .and_then(Value::as_str)
        .context("missing key")?;
    run_ok("xdotool", &["key", "--", key]).await?;
    Ok(text(format!("sent key {key}")))
}

async fn launch_app(args: Value) -> Result<Value> {
    let cmdline = args
        .get("command")
        .and_then(Value::as_str)
        .context("missing command")?;
    let mut parts = shell_words(cmdline)?;
    if parts.is_empty() {
        bail!("empty command");
    }
    let program = parts.remove(0);
    let child = Command::new(&program)
        .args(&parts)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("failed to launch {program}"))?;
    Ok(text(format!("launched {program} (pid {:?})", child.id())))
}

async fn list_windows() -> Result<Value> {
    require_bin("wmctrl")?;
    let out = run_ok("wmctrl", &["-l"]).await?;
    Ok(text(out))
}

async fn focus_window(args: Value) -> Result<Value> {
    require_bin("wmctrl")?;
    let title = args
        .get("title")
        .and_then(Value::as_str)
        .context("missing title")?;
    run_ok("wmctrl", &["-a", title]).await?;
    Ok(text(format!("focused window matching '{title}'")))
}

fn shell_words(s: &str) -> Result<Vec<String>> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quote = None;
    for c in s.chars() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (None, '\'') | (None, '"') => quote = Some(c),
            (None, c) if c.is_whitespace() => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            (_, c) => cur.push(c),
        }
    }
    if quote.is_some() {
        bail!("unbalanced quote in: {s}");
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    Ok(out)
}

fn tempfile_path(ext: &str) -> String {
    let dir = std::env::temp_dir();
    let name = format!(
        "mcp-linux-control-{}-{}.{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0),
        ext
    );
    dir.join(name).to_string_lossy().into_owned()
}
