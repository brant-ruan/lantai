#!/usr/bin/env bash
# Builds the `aj` command-line tool for the app bundle and leaves it at target/aj-bundle/aj,
# where tauri.conf.json (bundle.macOS.files) picks it up as Lantai.app/Contents/MacOS/aj.
# For a universal build it builds both architectures and merges them with lipo.
# The binary is signed ad hoc so the app bundle's own ad-hoc signature stays valid.
#
# Runs as part of Tauri's beforeBuildCommand. The target comes from AJ_BUNDLE_TARGET, else
# TAURI_ENV_TARGET_TRIPLE (set by the Tauri CLI), else the host.
set -euo pipefail

cd "$(dirname "$0")/.."

TARGET="${AJ_BUNDLE_TARGET:-${TAURI_ENV_TARGET_TRIPLE:-}}"
OUT_DIR="target/aj-bundle"
mkdir -p "$OUT_DIR"

build_one() {
  cargo build --release --locked -p aj-cli --target "$1" >&2
  echo "target/$1/release/aj"
}

case "$TARGET" in
  universal-apple-darwin)
    arm="$(build_one aarch64-apple-darwin)"
    x86="$(build_one x86_64-apple-darwin)"
    lipo -create -output "$OUT_DIR/aj" "$arm" "$x86"
    ;;
  "")
    cargo build --release --locked -p aj-cli
    cp target/release/aj "$OUT_DIR/aj"
    ;;
  *)
    cp "$(build_one "$TARGET")" "$OUT_DIR/aj"
    ;;
esac

codesign --force --sign - "$OUT_DIR/aj"
echo "bundled aj: $(lipo -archs "$OUT_DIR/aj")"
