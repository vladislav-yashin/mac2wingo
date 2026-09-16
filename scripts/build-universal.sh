#!/usr/bin/env bash
#
# Build a universal (Apple Silicon + Intel) Mac2Win Go .app bundle.
#
# Usage: ./scripts/build-universal.sh
#
set -euo pipefail

APP_NAME="Mac2Win Go"
BIN_NAME="mac2wingo"
BUNDLE_ID="dev.mac2wingo.Mac2WinGo"
VERSION="$(grep -m1 '^version' Cargo.toml | sed -E 's/.*"([^"]+)".*/\1/')"
# Override with PROFILE=dev for a quick, unoptimised bundle (used in CI smoke tests).
PROFILE="${PROFILE:-release}"
if [ "$PROFILE" = "dev" ]; then
    CARGO_PROFILE_FLAG=""
    PROFILE_DIR="debug"
else
    CARGO_PROFILE_FLAG="--profile $PROFILE"
    PROFILE_DIR="$PROFILE"
fi

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

DIST_DIR="$ROOT_DIR/dist"
APP_DIR="$DIST_DIR/$APP_NAME.app"
CONTENTS_DIR="$APP_DIR/Contents"
MACOS_DIR="$CONTENTS_DIR/MacOS"
RESOURCES_DIR="$CONTENTS_DIR/Resources"

ARM_TARGET="aarch64-apple-darwin"
INTEL_TARGET="x86_64-apple-darwin"

echo "==> Ensuring Rust targets are installed"
rustup target add "$ARM_TARGET" "$INTEL_TARGET"

echo "==> Building binaries (profile: $PROFILE)"
cargo build $CARGO_PROFILE_FLAG --target "$ARM_TARGET"
cargo build $CARGO_PROFILE_FLAG --target "$INTEL_TARGET"

if [ ! -x "$ROOT_DIR/vendor/tools/wimlib-imagex" ] && [ "${FETCH_TOOLS:-0}" = "1" ]; then
    echo "==> Building bundled helper tools"
    "$ROOT_DIR/scripts/fetch-tools.sh"
fi

echo "==> Creating universal binary"
rm -rf "$APP_DIR"
mkdir -p "$MACOS_DIR" "$RESOURCES_DIR"

if [ -d "$ROOT_DIR/vendor/tools" ] && [ -n "$(ls -A "$ROOT_DIR/vendor/tools" 2>/dev/null | grep -v README.md || true)" ]; then
    echo "==> Bundling helper tools"
    mkdir -p "$RESOURCES_DIR/tools"
    cp -R "$ROOT_DIR/vendor/tools/." "$RESOURCES_DIR/tools/"
    find "$RESOURCES_DIR/tools" -type f -exec chmod +x {} +
else
    echo "WARNING: vendor/tools is empty; the produced app will not be able to write drives." >&2
    echo "         Run ./scripts/fetch-tools.sh (or re-run with FETCH_TOOLS=1) first." >&2
fi

echo "==> Bundling license texts"
mkdir -p "$RESOURCES_DIR/licenses"
cp "$ROOT_DIR/LICENSE" "$RESOURCES_DIR/licenses/MIT.txt"
cp "$ROOT_DIR/THIRD_PARTY_NOTICES.md" "$RESOURCES_DIR/licenses/"
cp -R "$ROOT_DIR/licenses/." "$RESOURCES_DIR/licenses/"
if [ ! -d "$ROOT_DIR/vendor/tools/licenses" ]; then
    echo "WARNING: vendor/tools/licenses is missing; per-tool license texts will not ship." >&2
    echo "         Re-run ./scripts/fetch-tools.sh to build them." >&2
fi

lipo -create \
    "target/$ARM_TARGET/$PROFILE_DIR/$BIN_NAME" \
    "target/$INTEL_TARGET/$PROFILE_DIR/$BIN_NAME" \
    -output "$MACOS_DIR/$BIN_NAME"

chmod +x "$MACOS_DIR/$BIN_NAME"

ICON_KEY=""
if [ -f "$ROOT_DIR/assets/AppIcon.icns" ]; then
    echo "==> Embedding app icon"
    cp "$ROOT_DIR/assets/AppIcon.icns" "$RESOURCES_DIR/AppIcon.icns"
    ICON_KEY="    <key>CFBundleIconFile</key>
    <string>AppIcon</string>
"
fi

echo "==> Writing Info.plist"
cat > "$CONTENTS_DIR/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>
    <string>$APP_NAME</string>
    <key>CFBundleDisplayName</key>
    <string>$APP_NAME</string>
$ICON_KEY    <key>CFBundleIdentifier</key>
    <string>$BUNDLE_ID</string>
    <key>CFBundleVersion</key>
    <string>$VERSION</string>
    <key>CFBundleShortVersionString</key>
    <string>$VERSION</string>
    <key>CFBundleExecutable</key>
    <string>$BIN_NAME</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>LSMinimumSystemVersion</key>
    <string>11.0</string>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>LSApplicationCategoryType</key>
    <string>public.app-category.utilities</string>
</dict>
</plist>
PLIST

if command -v codesign >/dev/null 2>&1; then
    echo "==> Ad-hoc code signing"
    codesign --force --deep --sign - "$APP_DIR" >/dev/null
fi

echo "==> Done: $APP_DIR"
echo "    $(lipo -archs "$MACOS_DIR/$BIN_NAME")"
