#!/usr/bin/env bash
#
# Cross-compile uv from Linux to macOS (aarch64 or x86_64).
#
# Prerequisites (Arch Linux):
#   sudo pacman -S clang lld zig
#   rustup target add aarch64-apple-darwin x86_64-apple-darwin
#
# Usage:
#   ./scripts/cross-build-macos/build.sh                    # aarch64 debug
#   ./scripts/cross-build-macos/build.sh x86_64             # x86_64 debug
#   ./scripts/cross-build-macos/build.sh aarch64 --release  # aarch64 release
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"

ARCH="${1:-aarch64}"
shift || true
EXTRA_CARGO_ARGS=("$@")

case "$ARCH" in
    aarch64|arm64) TARGET=aarch64-apple-darwin; ZIG_TARGET=aarch64-macos ;;
    x86_64|x64)    TARGET=x86_64-apple-darwin;  ZIG_TARGET=x86_64-macos  ;;
    *) echo "error: unsupported arch '$ARCH' (use aarch64 or x86_64)" >&2; exit 1 ;;
esac

for cmd in clang lld zig cargo; do
    if ! command -v "$cmd" &>/dev/null; then
        echo "error: '$cmd' not found in PATH" >&2; exit 1
    fi
done

ZIG_LIB_DIR="$(zig env | sed -n 's/.*lib_dir.*= "\(.*\)".*/\1/p')"

WORK_DIR="$REPO_DIR/target/cross-build-macos"
LIBS_DIR="$WORK_DIR/libs"

mkdir -p "$LIBS_DIR" "$WORK_DIR/bin"

# Point the linker at Zig's bundled libSystem.tbd (covers libc, libm, etc.).
ln -sf "$ZIG_LIB_DIR/libc/darwin/libSystem.tbd" "$LIBS_DIR/libSystem.tbd"
ln -sf libSystem.tbd "$LIBS_DIR/libc.tbd"
ln -sf libSystem.tbd "$LIBS_DIR/libm.tbd"

# libSystem.tbd only covers /usr/lib/libSystem.B.dylib and its sub-libraries.
# Frameworks like CoreFoundation, Security, etc. need their own .tbd stubs so
# the linker can resolve -framework flags.
for f in "$SCRIPT_DIR"/tbd/*.tbd; do
    cp "$f" "$LIBS_DIR/"
done
cp "$SCRIPT_DIR/SDKSettings.json" "$LIBS_DIR/"

for fw in CoreFoundation Security SystemConfiguration Foundation; do
    mkdir -p "$LIBS_DIR/${fw}.framework"
    ln -sf "$LIBS_DIR/lib${fw}.tbd" "$LIBS_DIR/${fw}.framework/${fw}.tbd"
done

# The cc-rs crate passes --target=arm64-apple-macosx and -mmacosx-version-min
# flags that conflict with zig's -target flag.  The wrapper filters those out
# and injects the correct zig target plus a -isystem path for the stub
# syscall.h header that jemalloc needs.
cat > "$WORK_DIR/bin/zig-cc-$TARGET" <<ZIGCC
#!/bin/sh
args=""
skip_next=0
for arg in "\$@"; do
    if [ "\$skip_next" = "1" ]; then
        skip_next=0
        continue
    fi
    case "\$arg" in
        --target=*) continue ;;
        -target) skip_next=1; continue ;;
        -arch) skip_next=1; continue ;;
        -mmacosx-version-min=*) continue ;;
        *) args="\$args \$arg" ;;
    esac
done
exec zig cc -target $ZIG_TARGET -fno-sanitize=undefined -isystem "$SCRIPT_DIR/darwin-headers" \$args
ZIGCC
chmod +x "$WORK_DIR/bin/zig-cc-$TARGET"

cat > "$WORK_DIR/bin/zig-ar" <<'ZIGAR'
#!/bin/sh
exec zig ar "$@"
ZIGAR
chmod +x "$WORK_DIR/bin/zig-ar"

# Fake xcrun so rustc finds our SDK root without warnings.
cat > "$WORK_DIR/bin/xcrun" <<'XCRUN'
#!/bin/sh
while [ $# -gt 0 ]; do
    case "$1" in
        --show-sdk-path) echo "$SDKROOT"; exit 0 ;;
        --show-sdk-version) echo "14.0"; exit 0 ;;
        *) echo "fake xcrun: unsupported argument: $1" >&2; exit 1 ;;
    esac
    shift
done
XCRUN
chmod +x "$WORK_DIR/bin/xcrun"

# Use CARGO_TARGET_*_LINKER and CARGO_TARGET_*_RUSTFLAGS env vars to configure
# the cross-compilation without patching .cargo/config.toml.
TARGET_ENV="$(echo "$TARGET" | tr 'a-z-' 'A-Z_')"
RUSTFLAGS="-C link-arg=-fuse-ld=lld -C link-arg=--target=${TARGET} -C link-arg=-L${LIBS_DIR} -C link-arg=-F${LIBS_DIR}"

echo "==> Building uv for ${TARGET}..."
env \
    PATH="$WORK_DIR/bin:$PATH" \
    "CARGO_TARGET_${TARGET_ENV}_LINKER=clang" \
    "CARGO_TARGET_${TARGET_ENV}_RUSTFLAGS=$RUSTFLAGS" \
    "CC_${ARCH}_apple_darwin=$WORK_DIR/bin/zig-cc-$TARGET" \
    "AR_${ARCH}_apple_darwin=$WORK_DIR/bin/zig-ar" \
    SDKROOT="$LIBS_DIR" \
    cargo build --target "$TARGET" -p uv "${EXTRA_CARGO_ARGS[@]+"${EXTRA_CARGO_ARGS[@]}"}"

BINARY="$REPO_DIR/target/$TARGET/$(
    if [[ "${EXTRA_CARGO_ARGS[*]+"${EXTRA_CARGO_ARGS[*]}"}" == *--release* ]]; then
        echo release
    else
        echo debug
    fi
)/uv"

echo ""
echo "==> Done."
file "$BINARY"
