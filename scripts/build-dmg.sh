#!/usr/bin/env bash
#
# Package the built Mac2Win Go.app into a distributable DMG.
#
# Requires dist/Mac2Win Go.app (run ./scripts/build-universal.sh first).
#
# Usage: ./scripts/build-dmg.sh
#
# Env:
#   VERSION   override the version read from Cargo.toml
#
set -euo pipefail

APP_NAME="Mac2Win Go"
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

VERSION="${VERSION:-$(grep -m1 '^version' Cargo.toml | sed -E 's/.*"([^"]+)".*/\1/')}"

DIST_DIR="$ROOT_DIR/dist"
APP_DIR="$DIST_DIR/$APP_NAME.app"
DMG_NAME="Mac2WinGo-$VERSION.dmg"
DMG_PATH="$DIST_DIR/$DMG_NAME"

if [ ! -d "$APP_DIR" ]; then
    echo "error: $APP_DIR not found; run ./scripts/build-universal.sh first" >&2
    exit 1
fi

if ! command -v hdiutil >/dev/null 2>&1; then
    echo "error: hdiutil is required (only available on macOS)" >&2
    exit 1
fi

echo "==> Creating $DMG_NAME"
rm -f "$DMG_PATH"
hdiutil create \
    -volname "$APP_NAME" \
    -srcfolder "$APP_DIR" \
    -ov \
    -format UDZO \
    "$DMG_PATH" >/dev/null

echo "==> Done: $DMG_PATH"
echo "    $(du -h "$DMG_PATH" | cut -f1)"
