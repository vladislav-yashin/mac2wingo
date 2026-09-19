#!/usr/bin/env bash
#
# Generate the DMG installer background assets.
#
# Produces assets/dmg_background.png (1x) and assets/dmg_background@2x.png
# (2x, drawn by the Rust example); dmgbuild combines them into a HiDPI image.
#
# Usage: ./scripts/generate-dmg-background.sh
#
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

MASTER="assets/dmg_background@2x.png"
OUT="assets/dmg_background.png"

echo "==> Rendering DMG background"
cargo run --quiet --example gen_dmg_background

echo "==> Downscaling to 1x"
sips -z 420 660 "$MASTER" --out "$OUT" >/dev/null

echo "==> Done: $OUT"
