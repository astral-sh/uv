#!/usr/bin/env bash
# Install mold linker and make it the default linker.
#
# Retries on transient HTTP errors (e.g., 500) that the `rui314/setup-mold`
# GitHub Action does not handle.

set -euo pipefail

MOLD_VERSION="${MOLD_VERSION:-2.40.4}"

arch="$(uname -m)"
url="https://github.com/rui314/mold/releases/download/v${MOLD_VERSION}/mold-${MOLD_VERSION}-${arch}-linux.tar.gz"

if [ "$(whoami)" = root ]; then
    SUDO=""
else
    SUDO="sudo"
fi

echo "Installing mold ${MOLD_VERSION} (${arch})..."

wget -O- \
    --timeout=10 \
    --tries=5 \
    --waitretry=3 \
    --retry-connrefused \
    --retry-on-http-error=429,500,502,503,504 \
    --progress=dot:mega \
    "$url" \
    | $SUDO tar -C /usr/local --strip-components=1 --no-overwrite-dir -xzf -

# Make mold the default linker
current_ld="$(realpath /usr/bin/ld)"
if [ "$current_ld" != /usr/local/bin/mold ]; then
    $SUDO ln -sf /usr/local/bin/mold "$current_ld"
fi

echo "mold ${MOLD_VERSION} installed successfully"
mold --version
