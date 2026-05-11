# linux-coworker-ui

[![CI](https://github.com/jkhas8/linux-coworker-ui/actions/workflows/ci.yml/badge.svg)](../../actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Made for Linux](https://img.shields.io/badge/Made%20for-Linux-FCC624?logo=linux&logoColor=black)](#)

A Linux desktop GUI for the Claude Code agent — a Linux counterpart to
Anthropic's macOS/Windows-only **Claude Cowork**.

The app wraps `claude` (Claude Code CLI) as the agent loop and adds a Linux
desktop control layer (screenshot, click, type, launch apps) via a bundled MCP
server.

```
┌─────────────────────────────────────────────────────────┐
│  Tauri shell  ◀──── stream-json ────▶  claude (CLI)     │
│  (Rust + Solid)                            │            │
│                                            ▼            │
│                                   mcp-linux-control     │
│                                   (screenshot, xdotool, │
│                                    wmctrl, launch)      │
└─────────────────────────────────────────────────────────┘
```

## Prerequisites

System packages (Ubuntu 24.04 / Debian):

```sh
sudo apt install -y \
  libwebkit2gtk-4.1-dev \
  librsvg2-dev \
  libdbus-1-dev \
  libgtk-3-dev \
  libayatana-appindicator3-dev \
  libsoup-3.0-dev \
  pkg-config \
  build-essential \
  xdotool \
  wmctrl \
  maim
```

Toolchain:

- Rust >= 1.93
- Node >= 20 (or Bun)
- `claude` CLI on `$PATH` (Claude Code 2.x)

## Build

```sh
# 1. Build the MCP server
cargo build -p mcp-linux-control

# 2. Install frontend deps
bun install

# 3. Run the app (dev mode)
bun run tauri dev
```

Set `MCP_LINUX_CONTROL_BIN=/path/to/mcp-linux-control` if the auto-locator
can't find the binary (default is `target/debug/mcp-linux-control` relative
to the app binary).

## Layout

```
linux-coworker-ui/
├── Cargo.toml                       workspace root
├── package.json                     frontend (Vite + Solid + TS)
├── src/                             Solid frontend
│   ├── App.tsx                      chat UI shell
│   ├── stream.ts                    parse stream-json -> display blocks
│   └── types.ts
├── src-tauri/                       Tauri app
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs                   `run()` + Tauri commands
│       ├── agent.rs                 spawn/supervise `claude` subprocess
│       └── mcp_config.rs            write `.mcp.json` for `--mcp-config`
└── crates/
    └── mcp-linux-control/           MCP server: screenshot/xdotool/wmctrl
        └── src/main.rs
```

## How it works

1. The Tauri backend spawns `claude --print --input-format stream-json
   --output-format stream-json --mcp-config <generated>.json --permission-mode
   bypassPermissions`.
2. Each user message is written to claude's stdin as one NDJSON line of the
   form `{"type":"user","message":{...}}`.
3. claude streams NDJSON events back on stdout (assistant turns, tool calls,
   tool results). The backend forwards every line to the frontend via the
   `claude://event` Tauri event.
4. The frontend parses each event into display blocks (text bubble, tool-call
   card, tool-result card) and renders them.
5. When claude calls one of our MCP tools (e.g. `mcp__linux_control__screenshot`),
   the Claude Code CLI launches the `mcp-linux-control` binary as a stdio
   subprocess and routes the call there.

## MVP tools exposed by `mcp-linux-control`

| Tool           | Backed by           | Notes                                          |
|----------------|---------------------|------------------------------------------------|
| `screenshot`   | `maim` / `scrot`    | Returns base64 PNG image content to the model. |
| `xdo_click`    | `xdotool click`     | Optional `x`/`y`, optional `double`.           |
| `xdo_move`    | `xdotool mousemove` |                                                |
| `xdo_type`     | `xdotool type`      | Configurable per-char delay.                   |
| `xdo_key`      | `xdotool key`       | Accepts chords like `ctrl+s`.                  |
| `launch_app`   | `spawn()`           | Detached, returns immediately.                 |
| `list_windows` | `wmctrl -l`         |                                                |
| `focus_window` | `wmctrl -a`         | Substring match on title.                      |

File operations (Read/Write/Edit), shell (Bash), search (Grep/Glob) come for
free from Claude Code's built-in tools.

## Known limitations / next steps

- **Permission flow is YOLO right now.** Backend defaults to
  `--permission-mode bypassPermissions`. The "approve destructive only" UX
  needs a `--permission-prompt-tool` exposed by our MCP server that blocks on
  a Tauri-side approval dialog. See `agent.rs` -> `permission_mode`.
- **X11 only.** Screenshots, xdotool, and wmctrl rely on X11. Wayland support
  would swap in `grim`/`slurp`/`ydotool` and a portal-based screenshot path.
- **No session persistence.** Conversations are in-memory and reset on app
  close. Add `--resume <session-id>` plumbing.
- **No streaming partials.** We render finalized assistant messages only.
  Toggle `--include-partial-messages` and handle `stream_event` in
  `src/stream.ts` to get token-level streaming.

## Contributing

See [`docs/DEVELOPMENT.md`](docs/DEVELOPMENT.md) for architecture, protocols,
how to add a new MCP tool, the permission model roadmap, and the testing
plan.

## License

MIT.
