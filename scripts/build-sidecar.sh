#!/usr/bin/env bash
# Build the voco-mcp sidecar and stage it where Tauri's externalBin expects it:
# src-tauri/binaries/voco-mcp-<target-triple>. Tauri strips the triple suffix at
# bundle time, landing the binary at Contents/MacOS/voco-mcp in the .app.
#
# Runs from Tauri's beforeBuildCommand. Override the triple with SIDECAR_TARGET
# if you ever cross-compile (default: the host triple).
set -euo pipefail

cd "$(dirname "$0")/.."

TARGET="${SIDECAR_TARGET:-$(rustc -vV | sed -n 's/host: //p')}"
echo "build-sidecar: building voco-mcp for ${TARGET}"

cargo build --release -p voco-mcp \
  --manifest-path src-tauri/Cargo.toml \
  --target "${TARGET}"

mkdir -p src-tauri/binaries
cp "src-tauri/target/${TARGET}/release/voco-mcp" "src-tauri/binaries/voco-mcp-${TARGET}"
echo "build-sidecar: staged src-tauri/binaries/voco-mcp-${TARGET}"
