#!/usr/bin/env bash
# Install mold linker and make it the default linker.
#
# Retries on transient HTTP errors (e.g., 500) that the `rui314/setup-mold`
# GitHub Action does not handle.

set -euo pipefail

MOLD_VERSION="${MOLD_VERSION:-2.40.4}"

arch="$(uname -m)"

# Release assets are mutable, so new versions require reviewed SHA-256 digests.
case "${MOLD_VERSION}:${arch}" in
    2.40.4:aarch64)
        checksum="c799b9ccae8728793da2186718fbe53b76400a9da396184fac0c64aa3298ec37"
        ;;
    2.40.4:x86_64)
        checksum="4c999e19ffa31afa5aa429c679b665d5e2ca5a6b6832ad4b79668e8dcf3d8ec1"
        ;;
    *)
        echo "No trusted mold checksum for version ${MOLD_VERSION} (${arch})" >&2
        exit 1
        ;;
esac

url="https://github.com/rui314/mold/releases/download/v${MOLD_VERSION}/mold-${MOLD_VERSION}-${arch}-linux.tar.gz"

echo "Installing mold ${MOLD_VERSION} (${arch})..."

archive="$(mktemp)"
trap 'rm -f "$archive"' EXIT

wget -O "$archive" \
    --timeout=10 \
    --tries=5 \
    --waitretry=3 \
    --retry-connrefused \
    --retry-on-http-error=429,500,502,503,504 \
    --progress=dot:mega \
    "$url"

printf '%s  %s\n' "$checksum" "$archive" | sha256sum -c -

if [ "$(whoami)" = root ]; then
    SUDO=""
else
    SUDO="sudo"
fi

$SUDO tar -C /usr/local --strip-components=1 --no-overwrite-dir -xzf "$archive"

# Make mold the default linker
current_ld="$(realpath /usr/bin/ld)"
if [ "$current_ld" != /usr/local/bin/mold ]; then
    $SUDO ln -sf /usr/local/bin/mold "$current_ld"
fi

echo "mold ${MOLD_VERSION} installed successfully"
mold --version
