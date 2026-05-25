# Development Guide — linux-coworker-ui

This document is for people writing code in this repo. For end-user install
instructions see [`../README.md`](../README.md).

---

## 1. What we're building

A Linux desktop GUI that turns the **Claude Code CLI** into an agentic
desktop assistant — the Linux counterpart to Anthropic's macOS/Windows-only
**Claude Cowork**.

The application:

1. Spawns `claude` as a long-lived subprocess in stream-json mode.
2. Renders the conversation, tool calls, and tool results in a Solid-based
   chat UI hosted by a Tauri (Rust) shell.
3. Exposes Linux-desktop primitives (screenshot, mouse, keyboard, app launch,
   window management) to the agent as MCP tools via a bundled Rust binary
   (`mcp-linux-control`).

Goals: feel native on Linux, run from a single binary, behave like Cowork.
Non-goals (today): mobile, multi-user, web deployment.

---

## 2. High-level architecture

```
┌──────────────────────── linux-coworker-ui (Tauri window) ─────────────────┐
│                                                                            │
│   Solid frontend (src/)                                                    │
│     - composer + message log                                               │
│     - parses stream-json events into display blocks                        │
│     - sends `send_message` Tauri command                                   │
│                ▲                            │                              │
│                │ `claude://event`           │ invoke("send_message")       │
│                │ (Tauri event)              ▼                              │
│   Tauri Rust backend (src-tauri/src/)                                      │
│     - agent.rs:    spawn `claude --print --output-format stream-json`,     │
│                    forward stdout/stderr as Tauri events                   │
│     - mcp_config.rs: emit `.mcp.json` referencing the MCP server binary   │
│     - lib.rs:      Tauri commands `send_message`, `end_session`            │
│                                                                            │
└────────────────────────────────┬───────────────────────────────────────────┘
                                 │
                                 ▼ (stdin/stdout, NDJSON)
                         ┌───────────────────┐
                         │  claude (CLI)     │
                         │  stream-json mode │
                         └──┬────────┬───────┘
       Anthropic API ◀──────┘        │  tools/call (MCP, stdio)
                                     ▼
                         ┌───────────────────────┐
                         │ mcp-linux-control     │
                         │ (Rust binary)         │
                         │   screenshot, click,  │
                         │   type, key, launch,  │
                         │   windows…            │
                         └───────────────────────┘
```

Two data planes:

- **App ⇄ claude**: NDJSON over stdin/stdout, MCP-style "stream-json" events.
- **claude ⇄ mcp-linux-control**: NDJSON over stdin/stdout, MCP JSON-RPC.

---

## 3. Tech stack & rationale

| Layer | Choice | Why |
|---|---|---|
| Desktop shell | Tauri 2 (Rust) | Small binary, web frontend, native subprocess management. Cross-distro friendly. |
| Frontend | Solid + Vite + TS | Fine-grained reactivity, tiny bundle, familiar JSX. No virtual-DOM overhead. |
| Agent runtime | Claude Code CLI (`claude --print`) | Gets file/edit/grep/bash tools and the agent loop for free. |
| Desktop control | Custom Rust MCP server | Bridges to `xdotool`, `wmctrl`, `maim`/`scrot`. Stdio MCP, no extra runtime. |
| Markdown | `marked` + `DOMPurify` | GFM support, sanitized output. |
| Code highlight | `highlight.js/lib/core` (subset) | Per-language imports keep bundle ~150 KB. |

---

## 4. Repo layout

```
linux-coworker-ui/
├── Cargo.toml                       Cargo workspace root
├── package.json                     Frontend (Vite + Solid + TS)
├── tsconfig.json
├── vite.config.ts
├── index.html
├── README.md                        End-user install + run instructions
├── docs/
│   └── DEVELOPMENT.md               (this file)
├── src/                             Solid frontend source
│   ├── index.tsx                    Entry; imports hljs theme
│   ├── App.tsx                      Layout, message log, composer
│   ├── App.css                      Theme
│   ├── stream.ts                    raw event -> DisplayBlock[]
│   ├── markdown.ts                  marked + DOMPurify + hljs setup
│   ├── types.ts                     Shared TS types
│   └── components/
│       ├── Markdown.tsx
│       └── ToolCall.tsx             Tool call + tool result cards
├── src-tauri/                       Tauri Rust crate
│   ├── Cargo.toml
│   ├── tauri.conf.json              Window, identifier, build hooks
│   ├── capabilities/default.json    Frontend permissions
│   └── src/
│       ├── main.rs                  Thin entry calling `lib::run()`
│       ├── lib.rs                   Tauri commands + state
│       ├── agent.rs                 Subprocess supervisor
│       └── mcp_config.rs            Generate .mcp.json
└── crates/
    └── mcp-linux-control/           Standalone MCP server (Rust bin)
        ├── Cargo.toml
        └── src/main.rs              Stdio JSON-RPC + tool handlers
```

---

## 5. Local dev setup

### One-time

```sh
# System deps (Ubuntu 24.04 / Debian)
sudo apt install -y \
  libwebkit2gtk-4.1-dev librsvg2-dev libdbus-1-dev libgtk-3-dev \
  libayatana-appindicator3-dev libsoup-3.0-dev pkg-config build-essential \
  xdotool wmctrl maim

# Toolchain (assumed already installed)
#   - rustc >= 1.93 (rustup)
#   - node >= 20 or bun
#   - claude (Claude Code CLI) on PATH

# Frontend deps
bun install
```

### Dev loop

```sh
# 1. Build the MCP server (only when its code changes)
cargo build -p mcp-linux-control

# 2. Run the Tauri dev server (rebuilds on Rust+TS change)
bun run tauri dev
```

`bun run tauri dev` will:

- start Vite on http://localhost:1420 (frontend HMR),
- compile and run the Tauri shell (Rust hot-reload via cargo watcher).

The first run downloads the system libraries listed above and the full Tauri
crate tree — expect ~5 min on a cold build.

### Production build

```sh
cargo build -p mcp-linux-control --release
bun run tauri build
```

Outputs:
- App binary: `target/release/linux-coworker-ui`
- Bundles (AppImage / .deb): `src-tauri/target/release/bundle/`

---

## 6. The claude subprocess protocol

`src-tauri/src/agent.rs:43` invokes:

```
claude
  --print
  --input-format  stream-json
  --output-format stream-json
  --verbose
  --strict-mcp-config
  --mcp-config <generated>.json
  --permission-mode bypassPermissions
```

### Input (we write to its stdin)

One JSON object per line. We only emit user turns:

```json
{"type":"user","message":{"role":"user","content":[{"type":"text","text":"hello"}]}}
```

### Output (we read from its stdout)

Each line is one event. The shapes we currently handle (see `src/stream.ts`):

| `type` (+ `subtype`)    | Meaning                                  | Renders as |
|--------------------------|------------------------------------------|------------|
| `system`, `init`         | Session started                          | grey notice |
| `assistant`              | Assistant turn (text + tool_use blocks) | bubble + tool-call cards |
| `user` (with tool_result)| Tool execution result back to model      | result card |
| `result`, `success`/`error` | End of turn                          | grey notice; clears spinner |
| `stream_event`           | Partial deltas (not yet rendered)        | ignored |

Stderr lines are synthesized as `{type:"stderr", text:…}` by `agent.rs` and
unparseable lines as `{type:"unparsed", text:…}` so the frontend always sees
JSON.

### Why `--print` and not interactive?

Interactive mode owns the TTY. `--print` lets us pipe over plain stdio, which
is the only sane way to wrap it from Tauri.

---

## 7. The MCP server (`mcp-linux-control`)

Spec: MCP 2024-11-05, JSON-RPC 2.0 over stdio. Each line is one message.

Methods implemented in `crates/mcp-linux-control/src/main.rs`:

| Method | Purpose |
|---|---|
| `initialize` | Capability handshake. |
| `notifications/initialized` | Ack (no response). |
| `tools/list` | Return tool schemas. |
| `tools/call` | Dispatch to one of `screenshot`/`xdo_*`/`launch_app`/`list_windows`/`focus_window`. |
| `ping` | Keepalive. |

### Tool schema cheat sheet

Each tool is a JSON Schema describing inputs; outputs are `{content:[{type:"text"|"image",...}], isError:false}`.

For images we return `{type:"image", data:<base64>, mimeType:"image/png"}` —
the model receives this as a vision input.

### How `claude` discovers our server

`src-tauri/src/mcp_config.rs:14` writes a temp file like:

```json
{
  "mcpServers": {
    "linux_control": {
      "command": "/abs/path/to/mcp-linux-control",
      "args": [],
      "env": {}
    }
  }
}
```

…and `claude` is invoked with `--mcp-config <that-file> --strict-mcp-config`.
Tool names exposed to the model become `mcp__linux_control__<tool_name>`
(prefix stripped in the UI by `src/components/ToolCall.tsx:21`).

---

## 8. Adding a new MCP tool

End-to-end, 4 files:

1. **Schema** in `tool_definitions()` (`crates/mcp-linux-control/src/main.rs`):

   ```rust
   {
     "name": "copy_to_clipboard",
     "description": "Copy text to the X11 clipboard via xclip.",
     "inputSchema": {
       "type": "object",
       "required": ["text"],
       "properties": { "text": { "type": "string" } }
     }
   }
   ```

2. **Dispatch arm** in `call_tool()`:

   ```rust
   "copy_to_clipboard" => copy_to_clipboard(args).await?,
   ```

3. **Handler**:

   ```rust
   async fn copy_to_clipboard(args: Value) -> Result<Value> {
       require_bin("xclip")?;
       let t = args.get("text").and_then(Value::as_str).context("missing text")?;
       let mut child = Command::new("xclip")
           .args(["-selection", "clipboard"])
           .stdin(Stdio::piped())
           .spawn()?;
       child.stdin.take().unwrap().write_all(t.as_bytes()).await?;
       child.wait().await?;
       Ok(text("copied"))
   }
   ```

4. **(Optional)** Pretty rendering in `src/components/ToolCall.tsx`:
   - Add a friendly name to `NICE_NAME`.
   - Add a `<Match>` arm in the `Switch` if it deserves a one-line summary.

Run `cargo build -p mcp-linux-control` and restart the Tauri dev server — no
frontend reload needed unless you edited the TS files.

---

## 9. Frontend architecture

### Data flow

```
Tauri event "claude://event"   ─►  eventToBlocks(raw)  ─►  DisplayBlock[]
        (src/App.tsx:18)              (src/stream.ts)          │
                                                               ▼
                                                          <For each={...}>
                                                          BlockView (App.tsx)
                                                          ├── Markdown      (assistant text)
                                                          ├── ToolCallCard  (tool_use blocks)
                                                          ├── ToolResultCard (tool_result blocks)
                                                          ├── plain user bubble
                                                          └── system/error rows
```

### Components

| File | Role |
|---|---|
| `src/App.tsx` | App shell, signals (`blocks`, `input`, `busy`), event listener, composer. |
| `src/components/Markdown.tsx` | `<div innerHTML={...}>` wrapper. Memoizes parse. |
| `src/components/ToolCall.tsx` | Tool call + result cards with per-tool summaries. |
| `src/stream.ts` | Pure function from raw event → `DisplayBlock[]`. |
| `src/markdown.ts` | marked + DOMPurify + hljs configuration (side effects on import). |
| `src/types.ts` | Shared types. Loose by design — schema evolves. |

### State model

We don't use a store. A single `createSignal<DisplayBlock[]>([])` in
`App.tsx` is the source of truth, appended-only. This is fine while the log
fits in memory; if conversations get huge, swap to virtualized list +
windowed signal.

### `busy` clears when

- the agent emits a `result` event (end of turn), or
- the `send_message` invoke throws.

If the user sends a second prompt mid-turn, we currently still let it through
to claude's stdin and it queues. The spinner state may be misleading then;
fix on the roadmap.

---

## 10. Tauri commands (IPC)

Defined in `src-tauri/src/lib.rs`:

| Command | Args | Returns | Notes |
|---|---|---|---|
| `send_message` | `text: string, working_dir?: string, permission_mode?: string` | `session_id: string` | Lazily starts the agent on first call. |
| `end_session` | — | `()` | Closes stdin to claude; awaits subprocess exit. |

Events emitted by the backend:

| Event | Payload | Notes |
|---|---|---|
| `claude://event` | `{ session_id, raw }` | One per line of claude's stdout/stderr. |

---

## 11. Permission & safety model

**Current (MVP):** `--permission-mode bypassPermissions`. The agent runs
every tool without prompting. This is the YOLO path; suitable for
exploration only.

**Planned:** "Approve destructive actions only" — the UX the user signed
off on:

1. Allow read-only built-ins by default (`Read`, `Grep`, `Glob`, `WebFetch`
   …) via `--allowed-tools`.
2. For everything else, register a permission gate tool on the MCP side and
   pass `--permission-prompt-tool mcp__linux_control__permission_prompt`.
3. That tool **blocks** until the Tauri frontend resolves an Approve/Deny
   prompt rendered next to the tool-call card. Cross-process signalling will
   go through a Unix-domain socket the Tauri process owns; the MCP child
   inherits the path via env var.
4. Approval response shape mirrors the Claude Code spec:
   `{ behavior: "allow" | "deny", updatedInput?, message? }`.

Until that's done, anyone who launches the app is implicitly trusting the
model with full keyboard/mouse/filesystem control.

---

## 12. Roadmap

In rough priority order:

- [ ] **Permission prompt tool** (see §11). Unblocks shipping.
- [ ] **Session persistence** via `--resume <id>` and SQLite-backed history.
- [ ] **Token streaming** — handle `stream_event` deltas in `src/stream.ts`
      so assistant text appears progressively. Toggle via
      `--include-partial-messages` in `agent.rs`.
- [ ] **Edit tool diff view** — render `Edit` results as colored diffs
      (highlight.js `diff` language is already registered).
- [ ] **Wayland support** — abstract the desktop driver so we can swap
      `xdotool`/`maim`/`wmctrl` for `ydotool`/`grim`/portals.
- [ ] **Settings UI** — pick working directory, permission mode, model.
- [ ] **System tray + global hotkey** — invoke the assistant without
      focusing the window.
- [ ] **Multi-session tabs**.
- [ ] **AppImage/Flatpak release pipeline**.

---

## 13. Testing strategy

### Stack

- **Frontend**: [Vitest](https://vitest.dev/) + jsdom + Solid Testing
  Library. Tests live next to the file under test as
  `<name>.test.ts` / `<name>.test.tsx`.
- **Rust**: built-in `cargo test`. Unit tests live in a `#[cfg(test)] mod
  tests` at the bottom of each module; integration tests under
  `<crate>/tests/`.
- **Coverage**: V8 (frontend, via `@vitest/coverage-v8`) + LLVM source-
  based coverage (Rust, via
  [`cargo-llvm-cov`](https://github.com/taiki-e/cargo-llvm-cov)). Both
  emit lcov files.

### Commands

| Goal | Command |
|---|---|
| Run all frontend tests | `bun run test` |
| Watch frontend tests | `bun run test:watch` |
| Frontend tests + coverage | `bun run test:coverage` |
| Run all Rust tests | `cargo test --workspace` |
| Rust tests + coverage | `cargo llvm-cov --workspace --lcov --output-path coverage/rust.lcov.info` |

### Coverage policy

**New or modified code must reach >90% line and branch coverage.** The
gate is enforced in CI (`.github/workflows/ci.yml`, `diff-coverage-gate`
job) using
[`diff-cover`](https://github.com/Bachmann1234/diff_cover) against the
PR's merge base. A PR whose diff coverage drops below 90% will fail the
build with a list of uncovered lines.

The gate runs only on `pull_request` events — pushes to `main` are not
gated (the gate's job is to keep new work covered, not to retroactively
cover the existing codebase).

**Excluded from coverage** (treated as framework / subprocess glue, not
business logic):

- `src-tauri/src/lib.rs` and `src-tauri/src/main.rs` — Tauri entry
  points; commands declared here are thin wrappers around the modules
  below.
- `src-tauri/src/agent.rs` — orchestrates the `claude` subprocess and
  the stream-event reader. Needs integration tests (with a mocked
  subprocess), which are out of scope for the per-PR unit gate. Any
  pure helpers in this file (e.g. `build_user_message`) can still be
  unit-tested directly.
- Frontend: `src/vite-env.d.ts`, `src/index.tsx`, and `*.test.{ts,tsx}`
  files (configured in `vitest.config.ts`).

If you add business logic that genuinely belongs in one of these files,
move it into a sibling module and unit-test it there.

### Manual smoke test

When in doubt, run `bun run tauri dev` and exercise:

1. *"take a screenshot and tell me what app is focused"* — verify a
   `screenshot` tool card renders and the assistant reads the image.
2. *"create a file ~/tmp/hello.txt with 'hi'"* — verify the `Write` card
   renders; file exists on disk.
3. *"list windows"* — verify `mcp__linux_control__list_windows` renders.

---

## 14. Troubleshooting

| Symptom | Likely cause | Fix |
|---|---|---|
| `pkg-config exited with status code 1 … dbus-1` during `cargo build` | Missing system libs | Run the apt command in §5. |
| `claude: command not found` from the Tauri log | Claude Code CLI not on PATH | `npm i -g @anthropic-ai/claude-code` or add to PATH. |
| Tool calls return `xdotool not found` | Missing binary | `sudo apt install xdotool`. |
| Screenshot errors `install maim or scrot` | Missing binary | `sudo apt install maim`. |
| `mcp-linux-control` not located | Auto-locator can't find it | Set `MCP_LINUX_CONTROL_BIN=$PWD/target/debug/mcp-linux-control`. |
| Frontend bundle suddenly grows | A new highlight.js language imported as a side effect | Use `highlight.js/lib/languages/<x>` only; never `import "highlight.js"`. |
| MCP server logs not visible | We write to stderr | Run claude with `--verbose` (already on) and watch the dev console. |
| Window opens fullscreen on i3/sway | Tiling WM behavior, not the app | Add to your i3 config: `for_window [class="linux-coworker-ui"] floating enable` |

---

## 15. Glossary

- **MCP** — Model Context Protocol. Anthropic's spec for tool/resource
  servers. Stdio JSON-RPC variant is what we use.
- **stream-json** — Claude Code's NDJSON wire format for both input and
  output when run with `--print`.
- **tool_use / tool_result** — Anthropic message-content block types,
  emitted on the assistant turn (tool_use) and on the user turn following
  execution (tool_result).
- **Tauri command** — A Rust function exposed to the frontend, callable via
  `invoke(name, args)` from JS.
- **DisplayBlock** — Our local UI primitive for a single visible row in the
  conversation (text bubble, tool card, result card, …).

---

## 16. Conventions

- **No new docs.** Update this file or `README.md`. Don't sprout one-off
  markdown files for transient design notes; use commit messages and PR
  descriptions.
- **No new comments unless the *why* is non-obvious.** Names should carry
  the *what*.
- **Match existing styling** in `App.css`. We're sticking with a single
  flat theme file until it gets unwieldy.
- **Workspace deps** live in the root `Cargo.toml`; member crates pull them
  via `<crate>.workspace = true`.
- **Tool schemas are public API** (the model sees them). Edit them as
  carefully as you'd edit an HTTP API: keep descriptions tight, keep
  required fields minimal, prefer enum constraints over free-form strings.
