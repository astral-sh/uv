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

# Build the pinned toolchain image.
#
# In CI, `--load` and `--cache-to` are incompatible: `--load` forces the
# docker exporter which silently drops the cache export. Work around this
# by doing two passes: one to warm/export the GHA layer cache and one to
# load the image into the local daemon using the cache.
if [ "${CI:-}" = "true" ]; then
    docker buildx build \
        --cache-from type=gha \
        --cache-to type=gha,mode=max \
        -f "$TRAMPOLINE_DIR/Dockerfile" "$TRAMPOLINE_DIR" \
        "$@"
    docker buildx build -t uv-trampoline-builder --load \
        --cache-from type=gha \
        -f "$TRAMPOLINE_DIR/Dockerfile" "$TRAMPOLINE_DIR"
else
    docker buildx build -t uv-trampoline-builder --load \
        -f "$TRAMPOLINE_DIR/Dockerfile" "$TRAMPOLINE_DIR" \
        "$@"
fi

# Build trampolines inside the container with the workspace bind-mounted.
# The working directory must be the trampoline crate so cargo picks up
# .cargo/config.toml (which enables build-std).
docker run --rm \
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
