#!/bin/bash
# Build RustLab.icns from RustLab-1024.png (regenerate that with
# generate-icon.py). Uses sips + iconutil (both ship with macOS).
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
SRC="$HERE/RustLab-1024.png"
SET="$HERE/RustLab.iconset"

[ -f "$SRC" ] || { echo "error: $SRC missing — run generate-icon.py first"; exit 1; }

rm -rf "$SET"
mkdir "$SET"
for size in 16 32 128 256 512; do
    sips -z $size $size "$SRC" --out "$SET/icon_${size}x${size}.png" >/dev/null
    sips -z $((size * 2)) $((size * 2)) "$SRC" --out "$SET/icon_${size}x${size}@2x.png" >/dev/null
done

iconutil -c icns "$SET" -o "$HERE/RustLab.icns"
rm -rf "$SET"
echo "wrote $HERE/RustLab.icns"
