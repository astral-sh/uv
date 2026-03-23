#!/usr/bin/env bash
set -euxo pipefail

# Install uv into the Termux prefix
cp /uv /data/data/com.termux/files/usr/bin/uv
chmod +x /data/data/com.termux/files/usr/bin/uv

# Test uv
uv --version

# Termux uses Bionic libc (not glibc or musl), so uv cannot discover
# managed Python installations. Use only-system to skip that check.
export UV_PYTHON_PREFERENCE=only-system
uv python find
uv run -- python --version
