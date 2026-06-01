#!/usr/bin/env sh
## Install system requirements for cross-compiling Linux wheels in a manylinux container.
##
## Usage:
##
##   $ scripts/install-manylinux-build-requirements.sh <target>

set -eu

TARGET="${1:?Usage: scripts/install-manylinux-build-requirements.sh <target>}"

# Install the cross target on the 64-bit container (noop if it is already installed).
rustup target add "$TARGET"

if command -v yum >/dev/null 2>&1; then
    yum update -y
    yum install -y pkgconfig libatomic

    if [ "$TARGET" = "i686-unknown-linux-gnu" ]; then
        yum install -y glibc-devel.i686 libstdc++-devel.i686 libatomic.i686
    fi

    # Symlink libatomic so the linker can find it with -latomic.
    if [ -f "/usr/lib/libatomic.so.1" ] && [ ! -f "/usr/lib/libatomic.so" ]; then
        ln -s /usr/lib/libatomic.so.1 /usr/lib/libatomic.so
    fi
else
    apt update -y
    apt-get install -y pkg-config
fi
