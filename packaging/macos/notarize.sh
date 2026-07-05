#!/bin/bash
# Notarize and staple the DMG produced by build-installer.sh.
#
# One-time setup (stores Apple ID credentials in the keychain):
#   xcrun notarytool store-credentials rustlab-notary \
#     --apple-id you@example.com --team-id YOURTEAMID
#
# Usage:
#   NOTARY_PROFILE=rustlab-notary ./packaging/macos/notarize.sh
set -euo pipefail

PROFILE="${NOTARY_PROFILE:?set NOTARY_PROFILE to your notarytool keychain profile name}"

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
DIST="$ROOT/dist"
VERSION="$(grep -m1 '^version' "$ROOT/Cargo.toml" | cut -d'"' -f2)"
DMG="$DIST/RustLab-$VERSION.dmg"

[ -f "$DMG" ] || { echo "error: $DMG not found — run build-installer.sh first"; exit 1; }

echo "==> Submitting $DMG for notarization (waits for Apple)"
xcrun notarytool submit "$DMG" --keychain-profile "$PROFILE" --wait

echo "==> Stapling ticket"
xcrun stapler staple "$DMG"
xcrun stapler validate "$DMG"

echo "Done. $DMG is signed, notarized, and stapled."
