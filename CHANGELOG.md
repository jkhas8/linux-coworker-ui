# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Attachments now accept more than just images. Drag-drop or the 📎 picker
  takes PDFs (sent as Anthropic `document` content blocks), text/code files
  (decoded as UTF-8 and inlined as a fenced text block with a filename
  header), and spreadsheets (`.xlsx`, `.xls`, `.xlsm`, `.xlsb`, `.ods`,
  `.fods`) which are parsed client-side via SheetJS and converted to CSV per
  sheet before being inlined as text.
- Non-image attachments render as a file chip (extension badge + filename +
  size) both in the composer strip and in the chat log.

### Changed
- The `Attachment` type is now a discriminated union (`image | pdf | text`).
  The Tauri backend's `UserAttachment` is a matching tagged enum; the
  message builder dispatches per kind to emit the right Anthropic content
  block (`image`, `document`, or `text`).
- Unsupported-file error message now lists what *is* supported instead of
  just naming the rejected MIME type.

### Fixed
- Clicking **+ New** to start a fresh conversation now also closes the
  split-pane file preview. Previously the right-side preview panel stayed
  open and orphaned, still showing whichever file the previous conversation
  had opened.

## [0.4.0] - 2026-05-12

### Added
- Split-pane file preview: clicking a new **Preview** button on a Read /
  Write / Edit / MultiEdit / NotebookEdit tool-call card opens the target
  file in a right-side panel.
- Preview header has Code / Preview toggle (markdown only), reload-from-disk
  (`↻`), and close (`×`) controls.
- Code view with highlight.js syntax highlighting, per-line numbers in a
  CSS-counter gutter, per-line indent guides drawn only across each line's
  leading-whitespace zone, and soft-wrap on overflow (no horizontal scroll).
- Auto-detected indent unit (2 vs 4 spaces) drives both `tab-size` and the
  indent-guide spacing.
- `splitHighlightedLines` helper closes/reopens hljs spans at newlines so
  multi-line constructs (block comments, template literals) stay valid HTML
  per-line.
- Markdown preview supports inline **Mermaid** diagrams via fenced
  ` ```mermaid ` blocks; rendered as SVG by mermaid.run with a dark theme
  and an error fallback showing the parser message + source.
- Re-clicking Preview on the same file bumps a refresh counter so the
  panel re-reads from disk (helpful after another Edit lands).

### Backend
- New `read_file(path)` Tauri command. Reads any path; rejects non-UTF-8
  files with a friendly message instead of trying to render bytes.

## [0.3.0] - 2026-05-12

### Added
- Send button doubles as Stop while a turn is running; clicking it kills the
  in-flight `claude` subprocess while keeping the conversation's session id.
- Inline `stopped` badge + Retry button on the right of a cancelled user
  message. Retry re-sends the same text + attachments.
- AskUserQuestion built-in tool now renders as an interactive form (radio /
  checkbox per question, "Other" free-text input, submit-and-lock). The
  phantom error tool_result it emits in `--print` mode is hidden.
- Assistant answers with leading `>` blockquote reasoning collapse that
  prefix behind a "Show reasoning" toggle — only the final answer is visible
  by default.
- Tool result cards collapse by default with a one-line preview + line count.
- Screenshots downsample to 25% by default in `mcp-linux-control`
  (configurable via the `scale` arg), cutting image-token cost ~5×.
- New `xdo_hover` MCP tool: moves the cursor and dwells so tooltips/hover
  states render before the model decides to click.
- Recognise Anthropic's hosted `server_tool_use` and `web_search_tool_result`
  content blocks; render web-search results as a clickable markdown list.
- Default case for unknown content blocks renders a visible `[unhandled X]`
  system row and logs the payload to the devtools console.

### Changed
- Split `AgentSession::end` into `cancel_turn` (keep session id, for the Stop
  button) and `end` (clear session id, for "+ New").
- Stream parser no longer drops content from user-role messages — tool_call
  echoes from `--resume` survive, so the blue/purple tool-call cards stay
  visible.
- Toned-down inline blockquote styling inside assistant markdown.

### Fixed
- Tool-call cards were getting vertically clipped due to `overflow: hidden`
  on `.tool`. Removed the clip and gave `.tool-head` an explicit min-height.
- Screenshot tool results that exceeded webkit2gtk's `data:` URL size limit
  silently rendered as a broken-image icon. Now go through
  `URL.createObjectURL(new Blob(...))` and accept both the MCP and Anthropic
  image-block shapes.

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
