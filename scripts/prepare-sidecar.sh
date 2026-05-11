#!/usr/bin/env bash
# Build the mcp-linux-control crate in release mode and copy the resulting
# binary into src-tauri/binaries/ with the host-triple suffix that Tauri's
# `externalBin` mechanism expects.
#
# Run before `tauri build` (or wire into `beforeBuildCommand`).

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

TARGET="$(rustc -vV | sed -n 's|host: ||p')"
if [ -z "$TARGET" ]; then
  echo "could not determine host target triple from rustc -vV" >&2
  exit 1
fi

cargo build -p mcp-linux-control --release

mkdir -p src-tauri/binaries
cp -f target/release/mcp-linux-control \
   "src-tauri/binaries/mcp-linux-control-${TARGET}"

echo "wrote src-tauri/binaries/mcp-linux-control-${TARGET}"
