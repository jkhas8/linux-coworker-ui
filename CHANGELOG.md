# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.1] - 2026-05-12

### Fixed
- `beforeBuildCommand` in `tauri.conf.json` now anchors to the git repo root
  via `git rev-parse --show-toplevel` instead of a relative `../scripts/...`
  path. The relative path resolved locally (cwd = `src-tauri/`) but failed in
  the `tauri-action` CI runner (cwd = project root), breaking the v0.2.0
  release workflow.

## [0.2.0] - 2026-05-11

### Added
- Image attachments in the composer: paste from clipboard (with async Clipboard
  API fallback), drag-and-drop, and file picker. Thumbnails show in a strip
  above the textarea; images are sent as Anthropic image content blocks.
- Markdown rendering for assistant messages (marked + DOMPurify) with
  syntax-highlighted code blocks (highlight.js, subset of languages).
- Thinking blocks: collapsible card with custom toggle, monospace muted body
  rendered as preserved-whitespace plain text.
- "+ New" conversation button — kills the in-flight subprocess, clears the
  log, and starts a fresh Claude session on the next message.
- Inline image rendering for tool results (screenshot tool).
- `server_tool_use` / `web_search_tool_result` handling so Anthropic-hosted
  web search renders as a normal tool card + clickable result list.
- Interactive `AskUserQuestion` form with radio/checkbox options, "Other:"
  free-text input, and submit-and-lock behavior.
- Responsive layout using a `--gutter` CSS variable; chat now fills the window
  with a 1100 px content cap. Mobile breakpoint at 640 px collapses the role
  gutter.
- Dev-tools enabled in release builds (`tauri = { features = ["devtools"] }`).
- `scripts/prepare-sidecar.sh` builds and stages the `mcp-linux-control`
  binary for Tauri's `externalBin`; wired into `beforeBuildCommand`.

### Changed
- Agent runtime now spawns a fresh `claude` subprocess per user turn and
  passes `--resume <session_id>` from turn 2 onward. The previous long-lived
  stdin pipe didn't reliably carry multi-turn input.
- `mcp-linux-control` binary is bundled into the `.deb` as a Tauri sidecar
  (`externalBin`), so the installed app is self-contained with no env-var
  workaround.
- Filter user-role echo messages out of the stream (we already render the
  user's input locally; echoes were creating duplicate/red-error rows).
- `end_session` now kills the subprocess with a 500ms grace period instead
  of waiting indefinitely.

### Fixed
- White flash on cold start: inlined dark background in `index.html`.
- New-conversation button no longer hangs when the subprocess is mid-turn.

## [0.1.0] - 2026-05-11

### Added
- Initial Tauri + Solid + TypeScript scaffold.
- Cargo workspace with `src-tauri` (app) and `crates/mcp-linux-control` (MCP server).
- `mcp-linux-control` MCP server (stdio JSON-RPC 2.0) exposing:
  `screenshot`, `xdo_click`, `xdo_move`, `xdo_type`, `xdo_key`, `launch_app`,
  `list_windows`, `focus_window`.
- Agent subprocess supervisor (`src-tauri/src/agent.rs`) wrapping the
  Claude Code CLI in stream-json mode.
- Solid chat UI with markdown rendering, syntax-highlighted code blocks,
  collapsible tool-call cards, inline screenshot images, and a dark theme.
- Development guide (`docs/DEVELOPMENT.md`), README, CONTRIBUTING,
  Code of Conduct, Security policy, MIT License.
- GitHub Actions CI: Rust check + clippy + frontend typecheck + Vite build.
- GitHub Actions Release workflow: builds AppImage / `.deb` / `.rpm` on
  tag push and opens a draft GitHub release.

### Known limitations
- Permission flow is YOLO (`--permission-mode bypassPermissions`); the
  approval-dialog UX is on the roadmap.
- X11 only; Wayland support pending.
- No session persistence.
- No streaming token-by-token rendering.
