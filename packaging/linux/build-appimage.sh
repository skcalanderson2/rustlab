#!/bin/bash
# Build the RustLab AppImage with linuxdeploy. Called by build-packages.sh,
# also runnable standalone after `cargo build --release`.
#
# Downloads linuxdeploy + its appimage plugin on first run (to packaging/linux/tools).
# Requires FUSE2 to run the resulting AppImage on some distros
# (Ubuntu 24.04: apt install libfuse2t64), or run it with --appimage-extract-and-run.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
PKG="$ROOT/packaging/linux"
TOOLS="$PKG/tools"
VERSION="$(grep -m1 '^version' "$ROOT/Cargo.toml" | cut -d'"' -f2)"
ARCH="$(uname -m)"

[ -f "$ROOT/target/release/rustlab" ] || { echo "error: build first (cargo build --release)"; exit 1; }

mkdir -p "$TOOLS" "$ROOT/dist"
for tool in linuxdeploy linuxdeploy-plugin-appimage; do
    if [ ! -x "$TOOLS/$tool" ]; then
        echo "==> Downloading $tool"
        curl -fsSL -o "$TOOLS/$tool" \
            "https://github.com/linuxdeploy/$tool/releases/download/continuous/$tool-$ARCH.AppImage"
        chmod +x "$TOOLS/$tool"
    fi
done

APPDIR="$(mktemp -d)/AppDir"
export PATH="$TOOLS:$PATH"
export LINUXDEPLOY_OUTPUT_VERSION="$VERSION"

# --appimage-extract-and-run: works without FUSE (e.g. containers)
"$TOOLS/linuxdeploy" --appimage-extract-and-run \
    --appdir "$APPDIR" \
    --executable "$ROOT/target/release/rustlab" \
    --desktop-file "$PKG/rustlab.desktop" \
    --icon-file "$PKG/icons/512/rustlab.png" \
    --icon-filename rustlab \
    --output appimage

mv RustLab-"$VERSION"-"$ARCH".AppImage "$ROOT/dist/" 2>/dev/null \
    || mv rustlab-"$VERSION"-"$ARCH".AppImage "$ROOT/dist/" 2>/dev/null \
    || mv ./*.AppImage "$ROOT/dist/"
rm -rf "$(dirname "$APPDIR")"

echo "AppImage written to dist/"
