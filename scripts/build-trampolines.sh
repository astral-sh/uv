#!/bin/bash
# Build all Windows trampoline executables reproducibly using Docker.
#
# Extra arguments are forwarded to the docker build command

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
TRAMPOLINE_DIR="$REPO_ROOT/crates/uv-trampoline"
OUTPUT_DIR="$REPO_ROOT/crates/uv-trampoline-builder/trampolines"

# Read the nightly toolchain from rust-toolchain.toml
TOOLCHAIN="$(grep '^channel' "$TRAMPOLINE_DIR/rust-toolchain.toml" | sed 's/.*"\(.*\)"/\1/')"

# Pin to linux/amd64 so the container always matches the x86_64-unknown-linux-gnu
# toolchain hardcoded in the Dockerfile, regardless of the host architecture.
PLATFORM="linux/amd64"

# Build the pinned toolchain image.
docker buildx build -t uv-trampoline-builder --load \
    --platform "$PLATFORM" \
    -f "$TRAMPOLINE_DIR/Dockerfile" "$TRAMPOLINE_DIR" \
    "$@"

# Build trampolines inside the container with the workspace bind-mounted.
# The working directory must be the trampoline crate so cargo picks up
# .cargo/config.toml (which enables build-std).
docker run --rm \
    --platform "$PLATFORM" \
    -v "$REPO_ROOT:/workspace:ro" \
    -v "$OUTPUT_DIR:/output" \
    -w /workspace/crates/uv-trampoline \
    uv-trampoline-builder \
    bash -c '
        set -euo pipefail
        export CARGO_TARGET_DIR=/tmp/target

        cargo +"'"$TOOLCHAIN"'" xwin build --xwin-arch x86 --release --target i686-pc-windows-msvc
        cargo +"'"$TOOLCHAIN"'" xwin build --release --target x86_64-pc-windows-msvc
        cargo +"'"$TOOLCHAIN"'" xwin build --release --target aarch64-pc-windows-msvc

        for arch in i686 x86_64 aarch64; do
            for variant in console gui; do
                cp /tmp/target/$arch-pc-windows-msvc/release/uv-trampoline-$variant.exe \
                    /output/uv-trampoline-$arch-$variant.exe
            done
        done
    '

# Zero out non-deterministic PE fields (timestamps, debug GUIDs) so that
# builds are byte-for-byte reproducible despite LLVM non-determinism.
cargo run --quiet -p uv-trampoline-builder --bin normalize-pe-timestamps -- "$OUTPUT_DIR"/*.exe

echo "Done. Trampolines written to $OUTPUT_DIR"
