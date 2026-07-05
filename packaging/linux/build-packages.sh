#!/bin/bash
# Build Linux packages: .deb (cargo-deb) + AppImage (linuxdeploy).
#
# Run on a Linux machine from anywhere:
#   ./packaging/linux/build-packages.sh
#
# Prerequisites:
#   - Rust (rustup): https://rustup.rs
#   - build-essential, pkg-config (apt install build-essential pkg-config)
#   - Optional for the smoke test: python3 with ipykernel installed
#
# Note on compatibility: the .deb/AppImage require the glibc of the build
# machine or newer. Build on the oldest distro you want to support
# (Ubuntu 22.04 covers 22.04+/Debian 12+).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

command -v cargo >/dev/null || { echo "error: cargo not found — install via https://rustup.rs"; exit 1; }
command -v cc >/dev/null || { echo "error: no C compiler — apt install build-essential"; exit 1; }

VERSION="$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)"
mkdir -p dist

echo "==> Building release binary"
cargo build --release

echo "==> Headless kernel smoke test (best effort)"
if ! ./target/release/rustlab --headless-test; then
    echo "warning: headless test failed (no python kernel installed?) — continuing"
fi

echo "==> Building .deb (cargo-deb)"
cargo install cargo-deb --quiet 2>/dev/null || true
cargo deb --no-build
cp target/debian/*.deb dist/

echo "==> Building AppImage"
"$ROOT/packaging/linux/build-appimage.sh"

echo
echo "Done:"
ls -lh dist/*.deb dist/*.AppImage 2>/dev/null
