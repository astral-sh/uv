#!/usr/bin/env bash
set -euxo pipefail

# Install uv into the Termux prefix
cp /uv /data/data/com.termux/files/usr/bin/uv
chmod +x /data/data/com.termux/files/usr/bin/uv

# Install Python
pkg install -y python=3.13.12-5 \
    ca-certificates=1:2025.12.02 \
    gdbm=1.26-1 \
    libandroid-posix-semaphore=0.1-4 \
    libandroid-support=29-1 \
    libbz2=1.0.8-8 \
    libcrypt=0.2-6 \
    libexpat=2.7.4 \
    libffi=3.4.7-1 \
    liblzma=5.8.2 \
    libsqlite=3.52.0-1 \
    ncurses=6.6.20260124+really6.5.20250830 \
    ncurses-ui-libs=6.6.20260124+really6.5.20250830 \
    openssl=1:3.6.1 \
    readline=8.3.1-2 \
    zlib=1.3.2 \

# Test uv
uv --version

# Termux uses Bionic libc (not glibc or musl), so uv cannot discover
# managed Python installations. Use only-system to skip that check.
export UV_PYTHON_PREFERENCE=only-system
uv python find
uv run -- python --version
