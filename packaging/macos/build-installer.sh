#!/bin/bash
# Build RustLab.app and a signed DMG.
#
# Usage:
#   ./packaging/macos/build-installer.sh "Developer ID Application: Your Name (TEAMID)"
#
# The signing identity must exist in your keychain
# (`security find-identity -v -p codesigning` to list). Produces:
#   dist/RustLab.app
#   dist/RustLab-<version>.dmg   (signed; notarize with notarize.sh)
set -euo pipefail

IDENTITY="${1:?usage: build-installer.sh \"Developer ID Application: ... (TEAMID)\"}"

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
PKG="$ROOT/packaging/macos"
DIST="$ROOT/dist"
APP="$DIST/RustLab.app"
VERSION="$(grep -m1 '^version' "$ROOT/Cargo.toml" | cut -d'"' -f2)"
DMG="$DIST/RustLab-$VERSION.dmg"

echo "==> Building release binary"
cargo build --release --manifest-path "$ROOT/Cargo.toml"

echo "==> Assembling $APP"
rm -rf "$APP" "$DMG"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$ROOT/target/release/rustlab" "$APP/Contents/MacOS/rustlab"
cp "$PKG/Info.plist" "$APP/Contents/Info.plist"
if [ -f "$PKG/RustLab.icns" ]; then
    cp "$PKG/RustLab.icns" "$APP/Contents/Resources/RustLab.icns"
else
    echo "    (no RustLab.icns found — app will use the generic icon)"
fi

echo "==> Codesigning app (hardened runtime)"
codesign --force --options runtime --timestamp \
    --entitlements "$PKG/entitlements.plist" \
    --sign "$IDENTITY" \
    "$APP"
codesign --verify --deep --strict --verbose=2 "$APP"

echo "==> Creating DMG"
STAGE="$(mktemp -d)"
cp -R "$APP" "$STAGE/"
ln -s /Applications "$STAGE/Applications"
hdiutil create -volname "RustLab $VERSION" -srcfolder "$STAGE" -ov -format UDZO "$DMG"
rm -rf "$STAGE"

echo "==> Codesigning DMG"
codesign --force --timestamp --sign "$IDENTITY" "$DMG"

echo
echo "Done:"
echo "  $APP"
echo "  $DMG"
echo
echo "Next: notarize for Gatekeeper-clean installs:"
echo "  NOTARY_PROFILE=<profile> $PKG/notarize.sh"
