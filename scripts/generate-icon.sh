#!/usr/bin/env bash
#
# Generate assets/AppIcon.icns from the Rust-drawn master PNG.
#
# Usage: ./scripts/generate-icon.sh
#
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

MASTER="assets/icon_1024.png"
ICONSET="assets/AppIcon.iconset"
ICNS="assets/AppIcon.icns"

echo "==> Rendering master icon"
cargo run --quiet --example gen_icon

# A conveniently sized PNG is embedded in the binary and set as the runtime
# application icon (so the Dock shows it even when launched outside the bundle).
sips -z 512 512 "$MASTER" --out "assets/icon_512.png" >/dev/null

echo "==> Building iconset"
rm -rf "$ICONSET"
mkdir -p "$ICONSET"

# size:name pairs matching the iconset layout expected by iconutil.
SPECS=(
    "16 icon_16x16.png"
    "32 icon_16x16@2x.png"
    "32 icon_32x32.png"
    "64 icon_32x32@2x.png"
    "128 icon_128x128.png"
    "256 icon_128x128@2x.png"
    "256 icon_256x256.png"
    "512 icon_256x256@2x.png"
    "512 icon_512x512.png"
    "1024 icon_512x512@2x.png"
)

for spec in "${SPECS[@]}"; do
    size="${spec%% *}"
    name="${spec#* }"
    sips -z "$size" "$size" "$MASTER" --out "$ICONSET/$name" >/dev/null
done

echo "==> Writing $ICNS"
iconutil -c icns "$ICONSET" -o "$ICNS"
rm -rf "$ICONSET"

echo "==> Done: $ICNS"
