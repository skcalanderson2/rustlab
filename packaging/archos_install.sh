#!/bin/bash
# Install the RustLab AppImage for the current user on Arch-based distros
# (CachyOS, EndeavourOS, Manjaro, vanilla Arch).
#
# Copies the AppImage to ~/Applications, installs the icon, and creates a
# desktop menu entry. No root required.
#
# Usage:
#   ./packaging/archos_install.sh [path/to/RustLab-x.y.z-x86_64.AppImage]
#
# With no argument, uses the newest AppImage found in dist/.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"

# --- locate the AppImage --------------------------------------------------
if [ $# -ge 1 ]; then
    APPIMAGE="$1"
else
    APPIMAGE="$(ls -t "$ROOT"/dist/RustLab-*.AppImage 2>/dev/null | head -1 || true)"
fi
[ -n "${APPIMAGE:-}" ] && [ -f "$APPIMAGE" ] || {
    echo "error: no AppImage found — build one first (./packaging/linux/build-packages.sh)"
    echo "       or pass a path: $0 path/to/RustLab-*.AppImage"
    exit 1
}

# --- FUSE2 check (AppImages need it unless run with --appimage-extract-and-run)
if ! ldconfig -p 2>/dev/null | grep -q libfuse.so.2; then
    echo "warning: libfuse.so.2 not found — install it:  sudo pacman -S fuse2"
fi

# --- install --------------------------------------------------------------
APP_DIR="$HOME/Applications"
ICON_DIR="$HOME/.local/share/icons"
DESKTOP_DIR="$HOME/.local/share/applications"
mkdir -p "$APP_DIR" "$ICON_DIR" "$DESKTOP_DIR"

TARGET="$APP_DIR/$(basename "$APPIMAGE")"
echo "==> Installing $(basename "$APPIMAGE") to $APP_DIR"
cp "$APPIMAGE" "$TARGET"
chmod +x "$TARGET"

ICON_SRC="$ROOT/packaging/linux/icons/256/rustlab.png"
if [ -f "$ICON_SRC" ]; then
    cp "$ICON_SRC" "$ICON_DIR/rustlab.png"
    ICON_VALUE="$ICON_DIR/rustlab.png"
else
    echo "warning: icon not found at $ICON_SRC — menu entry will use a generic icon"
    ICON_VALUE="application-x-executable"
fi

echo "==> Creating desktop menu entry"
cat > "$DESKTOP_DIR/rustlab.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=RustLab
GenericName=Jupyter Notebook Editor
Comment=Native Jupyter notebook desktop app
Exec=$TARGET %f
Icon=$ICON_VALUE
Terminal=false
Categories=Development;Science;IDE;
MimeType=application/x-ipynb+json;
Keywords=jupyter;notebook;python;julia;kernel;
EOF

command -v update-desktop-database >/dev/null && update-desktop-database "$DESKTOP_DIR" || true

echo
echo "Installed:"
echo "  $TARGET"
echo "  $DESKTOP_DIR/rustlab.desktop"
echo
echo "RustLab should now appear in your application menu."
echo "Uninstall: rm '$TARGET' '$DESKTOP_DIR/rustlab.desktop' '$ICON_DIR/rustlab.png'"
