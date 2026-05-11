// Writes the .mcp.json file passed to `claude --mcp-config`, registering the
// bundled mcp-linux-control binary.

use anyhow::{Context, Result};
use serde_json::json;
use std::path::{Path, PathBuf};

pub fn write_default_config(server_binary: &Path) -> Result<PathBuf> {
    let dir = std::env::temp_dir().join(format!("linux-coworker-ui-{}", std::process::id()));
    std::fs::create_dir_all(&dir).context("create mcp config dir")?;
    let path = dir.join("mcp.json");
    let cfg = json!({
        "mcpServers": {
            "linux_control": {
                "command": server_binary.to_string_lossy(),
                "args": [],
                "env": {}
            }
        }
    });
    std::fs::write(&path, serde_json::to_vec_pretty(&cfg)?)?;
    Ok(path)
}

/// Locate the mcp-linux-control binary. In dev we look in
/// `target/debug` next to the workspace; users can override with the
/// `MCP_LINUX_CONTROL_BIN` env var.
pub fn locate_server_binary() -> Result<PathBuf> {
    if let Ok(p) = std::env::var("MCP_LINUX_CONTROL_BIN") {
        return Ok(PathBuf::from(p));
    }
    // Walk up from current exe to find target/<profile>/mcp-linux-control.
    let exe = std::env::current_exe()?;
    for ancestor in exe.ancestors() {
        for profile in ["debug", "release"] {
            let candidate = ancestor.join(profile).join("mcp-linux-control");
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
        let sibling = ancestor.join("mcp-linux-control");
        if sibling.is_file() {
            return Ok(sibling);
        }
    }
    anyhow::bail!(
        "could not locate mcp-linux-control binary. Build with `cargo build -p mcp-linux-control` \
         or set MCP_LINUX_CONTROL_BIN."
    )
}
