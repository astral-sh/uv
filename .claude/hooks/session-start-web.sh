#!/bin/bash
set -euo pipefail

# Install `gh`
if ! command -v gh &> /dev/null; then
    apt-get update -qq
    apt-get install -y -qq gh
fi

# Install clippy and rustfmt for the active toolchain.
rustup component add clippy rustfmt

# Set GH_REPO so `gh` works even when the git remote points to a local proxy
if [ -n "${CLAUDE_ENV_FILE:-}" ]; then
  echo 'export GH_REPO=astral-sh/uv' >> "$CLAUDE_ENV_FILE"
fi
