#!/usr/bin/env bash
set -euo pipefail

version="2026.06.24"
toolchain="srs-${version}"

case "$(uname -s)-$(uname -m)" in
    Darwin-arm64)
        target="aarch64-apple-darwin"
        checksum="4ad4717e66bf27e6645e78905a39983d83b5436a81006a4c6a990c6a0704ac8d"
        ;;
    Linux-x86_64)
        target="x86_64-unknown-linux-gnu"
        checksum="745c307f0081cfe2df27bfce1bad7004e50191e32625166fe488ecf15874989e"
        ;;
    *)
        echo "srs ${version} does not support $(uname -s)-$(uname -m)" >&2
        exit 1
        ;;
esac

archive="srs-${version}-${target}.tar.gz"
install_root="${RUNNER_TEMP:-${HOME}/code/tmp}/srs-toolchains"
snapshot="${install_root}/srs-${version}-${target}"

mkdir -p "$install_root"
curl \
    --proto '=https' \
    --tlsv1.2 \
    --retry 5 \
    --retry-all-errors \
    --location \
    --silent \
    --show-error \
    --fail \
    "https://github.com/zanieb/srs/releases/download/${version}/${archive}" \
    --output "${install_root}/${archive}"

actual_checksum="$(shasum -a 256 "${install_root}/${archive}" | cut -d ' ' -f 1)"
if [[ "$actual_checksum" != "$checksum" ]]; then
    echo "checksum mismatch for ${archive}: expected ${checksum}, got ${actual_checksum}" >&2
    exit 1
fi

tar -C "$install_root" -xzf "${install_root}/${archive}"
rustup toolchain link "$toolchain" "$snapshot"

rustc +"$toolchain" -Vv
cargo +"$toolchain" -Vv
cargo +"$toolchain" clippy -V

{
    echo "CARGO_INCREMENTAL=0"
    echo "RUSTUP_TOOLCHAIN=${toolchain}"
    echo "SRS_CARGO_ARTIFACT_CACHE_MAX_SIZE=4GiB"
} >> "$GITHUB_ENV"
