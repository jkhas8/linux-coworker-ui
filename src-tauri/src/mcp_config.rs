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

/// Locate the mcp-linux-control binary. Resolution order:
///   1. `MCP_LINUX_CONTROL_BIN` env var (explicit override).
///   2. Sibling of the running app binary (matches Tauri's `externalBin`
///      install layout in .deb / AppImage bundles).
///   3. Walk up ancestors looking for `target/<profile>/mcp-linux-control`
///      (dev workflow).
///   4. Fallback to `$PATH`.
pub fn locate_server_binary() -> Result<PathBuf> {
    if let Ok(p) = std::env::var("MCP_LINUX_CONTROL_BIN") {
        return Ok(PathBuf::from(p));
    }
    let exe = std::env::current_exe()?;

    // Tauri sidecars: same directory as the main binary, possibly with the
    // target-triple suffix kept in dev builds.
    if let Some(dir) = exe.parent() {
        for name in [
            "mcp-linux-control",
            "mcp-linux-control-x86_64-unknown-linux-gnu",
        ] {
            let p = dir.join(name);
            if p.is_file() {
                return Ok(p);
            }
        }
    }

    // Dev workspace layout.
    for ancestor in exe.ancestors() {
        for profile in ["debug", "release"] {
            let candidate = ancestor.join(profile).join("mcp-linux-control");
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }

    // PATH fallback.
    if let Ok(p) = which::which("mcp-linux-control") {
        return Ok(p);
    }

    anyhow::bail!(
        "could not locate mcp-linux-control binary. Build with `cargo build -p mcp-linux-control` \
         or set MCP_LINUX_CONTROL_BIN."
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn write_default_config_emits_parseable_json_with_binary_path() {
        let binary = PathBuf::from("/usr/local/bin/mcp-linux-control");
        let path = write_default_config(&binary).expect("write succeeds");
        let contents = std::fs::read_to_string(&path).expect("read back");
        let parsed: serde_json::Value =
            serde_json::from_str(&contents).expect("valid JSON on disk");
        assert_eq!(
            parsed["mcpServers"]["linux_control"]["command"],
            serde_json::Value::String(binary.to_string_lossy().into_owned()),
        );
    }

    #[test]
    fn locate_server_binary_respects_env_override() {
        // Safety: tests run single-threaded enough for this; if the test runner
        // ever parallelises this file, switch to a serial_test guard.
        unsafe { std::env::set_var("MCP_LINUX_CONTROL_BIN", "/tmp/fake-binary") };
        let resolved = locate_server_binary().expect("env override is honoured");
        assert_eq!(resolved, PathBuf::from("/tmp/fake-binary"));
        unsafe { std::env::remove_var("MCP_LINUX_CONTROL_BIN") };
    }
}
