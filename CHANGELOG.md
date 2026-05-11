# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
