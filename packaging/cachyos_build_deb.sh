#!/bin/bash
# Build Debian-compatible Linux packages (.deb + AppImage) on CachyOS/Arch.
#
# CachyOS runs bleeding-edge glibc, so binaries built natively won't run on
# Ubuntu 22.04/24.04 or Debian 12 — the audience for these packages. This
# script runs the normal Linux build (packaging/linux/build-packages.sh)
# inside an Ubuntu 22.04 container instead, giving the artifacts a glibc
# 2.35 floor.
#
# Usage (from anywhere, no root needed with podman; docker may need sudo):
#   ./packaging/cachyos_build_deb.sh
#
# Prerequisites: docker or podman
#   sudo pacman -S podman        # rootless, simplest
#   # or: sudo pacman -S docker && sudo systemctl enable --now docker
#
# Output lands in dist/ as usual. Upload with:
#   gh release upload v<version> dist/*.deb dist/*.AppImage -R skcalanderson2/rustlab
#
# Note: the container shares ./target with host builds. Rust fingerprints
# keep them from clashing, but if you see odd build errors after switching
# between host and container builds, run `cargo clean`.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"

# --- pick a container engine ----------------------------------------------
if command -v podman >/dev/null 2>&1; then
    ENGINE=podman
elif command -v docker >/dev/null 2>&1; then
    ENGINE=docker
    docker info >/dev/null 2>&1 || {
        echo "error: docker installed but daemon not running (or needs sudo):"
        echo "  sudo systemctl start docker"
        echo "  sudo usermod -aG docker \$USER   # then re-login, or run this script with sudo"
        exit 1
    }
else
    echo "error: need docker or podman:  sudo pacman -S podman"
    exit 1
fi
echo "==> Using $ENGINE with ubuntu:22.04 (glibc 2.35 floor)"

HOST_UID="$(id -u)"
HOST_GID="$(id -g)"

# Named volume caches the toolchain + crate downloads between runs.
$ENGINE run --rm \
    -v "$ROOT:/work" \
    -v rustlab-build-cache:/root/.cargo \
    -e HOST_UID="$HOST_UID" \
    -e HOST_GID="$HOST_GID" \
    -w /work \
    ubuntu:22.04 \
    bash -c '
        set -euo pipefail
        export DEBIAN_FRONTEND=noninteractive
        echo "==> [container] Installing build dependencies"
        apt-get update -qq
        apt-get install -y -qq build-essential pkg-config curl file ca-certificates git >/dev/null

        if ! command -v cargo >/dev/null 2>&1 && [ ! -x /root/.cargo/bin/cargo ]; then
            echo "==> [container] Installing Rust (cached in a volume for next time)"
            curl --proto "=https" --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y -q
        fi
        export PATH="/root/.cargo/bin:$PATH"

        echo "==> [container] Building packages"
        ./packaging/linux/build-packages.sh

        # Container runs as root; hand the outputs back to the host user.
        chown -R "$HOST_UID:$HOST_GID" /work/dist /work/target /work/packaging/linux/tools 2>/dev/null || true
    '

echo
echo "Done — Debian-compatible artifacts in dist/:"
ls -lh "$ROOT"/dist/*.deb "$ROOT"/dist/*.AppImage 2>/dev/null || true
