#!/usr/bin/env bash
# Build the non-macOS Python shims in the pinned container.
# Pass --check to compare with committed assets without changing them.
# Remaining arguments are passed to docker buildx build.
set -euo pipefail

if ! command -v docker >/dev/null 2>&1 && command -v podman >/dev/null 2>&1; then
    docker() { podman "$@"; }
fi

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
OUTPUT_DIR="$REPO_ROOT/crates/uv-trampoline-builder/python-shims"
CHECK_ARGS=()
OUTPUT_MODE=rw
if [[ "${1:-}" == "--check" ]]; then
    CHECK_ARGS=(--check)
    OUTPUT_MODE=ro
    shift
fi

docker buildx build --platform linux/amd64 --load -t uv-python-shim-builder \
    -f "$REPO_ROOT/crates/uv-python-shim/Dockerfile" \
    "$REPO_ROOT/crates/uv-python-shim" "$@"

docker run --rm --platform linux/amd64 \
    -v "$REPO_ROOT:/source:ro" \
    -v "$OUTPUT_DIR:/output:$OUTPUT_MODE" \
    uv-python-shim-builder \
    python3 /source/scripts/build-python-shims.py \
        --group cross --work-dir /work --freebsd-sysroot /opt/freebsd \
        --output-dir /output "${CHECK_ARGS[@]}"
