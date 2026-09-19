#!/usr/bin/env bash
#
# Package the built Mac2Win Go.app into a distributable, styled DMG.
#
# The image contains the app plus an "Applications" symlink and a Finder
# background/icon layout, so users can drag the app into /Applications like any
# standard macOS installer.
#
# Requires dist/Mac2Win Go.app (run ./scripts/build-universal.sh first) and
# assets/dmg_background.png (run ./scripts/generate-dmg-background.sh).
#
# Uses dmgbuild (https://github.com/dmgbuild/dmgbuild), which writes the
# Finder .DS_Store directly and therefore works on headless CI runners. If
# dmgbuild is not on PATH, it is installed into target/dmgbuild-venv.
#
# Usage: ./scripts/build-dmg.sh
#
# Env:
#   VERSION   override the version read from Cargo.toml
#   DMGBUILD  path to an existing dmgbuild executable
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

BACKGROUND="$ROOT_DIR/assets/dmg_background.png"
ICON="$ROOT_DIR/assets/AppIcon.icns"
SETTINGS="$ROOT_DIR/scripts/dmgbuild-settings.py"

if [ ! -d "$APP_DIR" ]; then
    echo "error: $APP_DIR not found; run ./scripts/build-universal.sh first" >&2
    exit 1
fi

if [ ! -f "$BACKGROUND" ]; then
    echo "error: $BACKGROUND not found; run ./scripts/generate-dmg-background.sh first" >&2
    exit 1
fi

if ! command -v hdiutil >/dev/null 2>&1; then
    echo "error: hdiutil is required (only available on macOS)" >&2
    exit 1
fi

# Resolve dmgbuild: explicit override, then PATH, then a project-local venv.
DMGBUILD="${DMGBUILD:-}"
if [ -z "$DMGBUILD" ] && command -v dmgbuild >/dev/null 2>&1; then
    DMGBUILD="$(command -v dmgbuild)"
fi
if [ -z "$DMGBUILD" ]; then
    VENV="$ROOT_DIR/target/dmgbuild-venv"
    if [ ! -x "$VENV/bin/dmgbuild" ]; then
        if ! command -v python3 >/dev/null 2>&1; then
            echo "error: dmgbuild not found and python3 is unavailable" >&2
            echo "       install it manually: pip install dmgbuild" >&2
            exit 1
        fi
        echo "==> Installing dmgbuild into $VENV"
        python3 -m venv "$VENV"
        "$VENV/bin/pip" install --quiet --disable-pip-version-check dmgbuild
    fi
    DMGBUILD="$VENV/bin/dmgbuild"
fi

DMGBUILD_ARGS=(
    -s "$SETTINGS"
    -D "app_path=$APP_DIR"
    -D "app_name=$APP_NAME.app"
    -D "background=$BACKGROUND"
)
if [ -f "$ICON" ]; then
    DMGBUILD_ARGS+=(-D "icon=$ICON")
fi

echo "==> Creating $DMG_NAME"
rm -f "$DMG_PATH"
"$DMGBUILD" "${DMGBUILD_ARGS[@]}" "$APP_NAME" "$DMG_PATH"

echo "==> Done: $DMG_PATH"
echo "    $(du -h "$DMG_PATH" | cut -f1)"
