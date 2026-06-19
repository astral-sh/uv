#!/bin/bash
# Build all Windows trampoline executables reproducibly using Docker.
#
# Extra arguments are forwarded to the docker build command

set -euo pipefail

if ! command -v docker >/dev/null 2>&1 && command -v podman >/dev/null 2>&1; then
    docker() { podman "$@"; }
fi

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
TRAMPOLINE_DIR="$REPO_ROOT/crates/uv-trampoline"
OUTPUT_DIR="$REPO_ROOT/crates/uv-trampoline-builder/trampolines"

# Read the nightly toolchain from `rust-toolchain.toml`.
TOOLCHAIN="$(grep '^channel' "$TRAMPOLINE_DIR/rust-toolchain.toml" | sed 's/.*"\(.*\)"/\1/')"

# Pin to linux/amd64 so the container always matches the x86_64-unknown-linux-gnu
# toolchain hardcoded in the Dockerfile, regardless of the host architecture.
PLATFORM="linux/amd64"

# Build the pinned toolchain image.
docker buildx build -t uv-trampoline-builder --load \
    --platform "$PLATFORM" \
    -f "$TRAMPOLINE_DIR/Dockerfile" "$TRAMPOLINE_DIR" \
    "$@"

# Build trampolines inside the container.
docker run --rm \
    --platform "$PLATFORM" \
    -v "$REPO_ROOT:/source:ro" \
    -v "$OUTPUT_DIR:/output" \
    -v "$SCRIPT_DIR/build-trampolines-in-docker.sh:/build-trampolines-in-docker.sh:ro" \
    uv-trampoline-builder \
    bash /build-trampolines-in-docker.sh "$TOOLCHAIN"

# Zero out non-deterministic PE fields (timestamps, debug GUIDs).
cargo run --quiet -p uv-trampoline-builder --bin normalize-pe-timestamps -- "$OUTPUT_DIR"/*.exe

echo "Done. Trampolines written to $OUTPUT_DIR"
