# Contributing to linux-coworker-ui

Thanks for your interest! This project is a Linux desktop GUI for the Claude
Code agent — see [`README.md`](README.md) for what it does and
[`docs/DEVELOPMENT.md`](docs/DEVELOPMENT.md) for the deep dive.

## Before you start

- Read [`docs/DEVELOPMENT.md`](docs/DEVELOPMENT.md) — especially
  §5 (dev setup) and §8 (adding a new MCP tool).
- For non-trivial changes, open an issue first to align on the approach.
- All participation is governed by the
  [Code of Conduct](CODE_OF_CONDUCT.md).

## Ways to contribute

- **Pick up an item from the roadmap** (`docs/DEVELOPMENT.md` §12).
- **Add a new MCP tool** — the 4-file walkthrough in §8 is the easiest entry
  point. Good first issues are tagged `good-first-issue`.
- **Fix a bug** from the issue tracker.
- **Improve docs** when something in this repo confused you.

## Dev loop

```sh
# One-time
sudo apt install -y libwebkit2gtk-4.1-dev librsvg2-dev libdbus-1-dev \
  libgtk-3-dev libayatana-appindicator3-dev libsoup-3.0-dev pkg-config \
  build-essential xdotool wmctrl maim
bun install

# Each session
cargo build -p mcp-linux-control
bun run tauri dev
```

## Before submitting a PR

```sh
# Rust
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test  --workspace

# Frontend
bun x tsc --noEmit
bun run build
```

CI (`.github/workflows/ci.yml`) runs the same checks on every PR.

## Commit & PR style

- **One logical change per PR.** Keep diffs reviewable.
- **Conventional Commits** for messages: `feat:`, `fix:`, `docs:`, `refactor:`,
  `chore:`, `ci:`, `test:`. Example: `feat(mcp): add copy_to_clipboard tool`.
- **Reference the issue** in the PR description (`Closes #123`).
- **Screenshots / GIFs** are welcome for any UI change.
- **No commented-out code, no `TODO: ...` without a tracking issue.**

## Coding conventions

- **Rust**: rustfmt defaults, clippy-clean. Prefer `anyhow::Result` for app
  code, surfaced as `Result<_, String>` only at Tauri command boundaries.
- **TypeScript**: Solid idioms (signals, `<For>`, `<Show>`). Keep components
  small; lift CSS into `App.css`.
- **No new top-level docs** — extend `README.md` or `docs/DEVELOPMENT.md`
  instead.
- **Comments**: only when the *why* isn't obvious. Names carry the *what*.

## Reporting bugs

Open a GitHub issue using the **Bug report** template. Include:

- Distro + version (`cat /etc/os-release | head -2`)
- Session type (`echo $XDG_SESSION_TYPE`)
- `claude --version`
- Steps to reproduce
- Logs (run `bun run tauri dev` from a terminal and paste the relevant
  excerpt — the agent's stderr appears there).

## Security issues

Please don't open public issues for security problems. See
[`SECURITY.md`](SECURITY.md).

## Releases (maintainers)

Releases are driven by tag pushes. To cut a release:

1. Update `CHANGELOG.md`: rename the `## [Unreleased]` heading to
   `## [vX.Y.Z] - YYYY-MM-DD` and add a fresh empty `Unreleased` section.
2. Bump versions in `Cargo.toml` (workspace) and `package.json`.
3. Commit, then tag and push:
   ```sh
   git tag vX.Y.Z
   git push origin main --tags
   ```
4. The `release.yml` workflow builds AppImage, `.deb`, and `.rpm` bundles
   on Ubuntu 22.04 (for broad glibc compatibility) and opens a **draft
   GitHub release**. Review it, paste the changelog body, and hit Publish.

Workflow runs are also available via **Actions → Release → Run workflow**
for dry-runs (leave the tag input blank — artifacts are uploaded but no
release is created).

## License

By contributing, you agree that your contributions will be licensed under
the project's [MIT License](LICENSE).
