#!/usr/bin/env bash
set -euo pipefail

version="2026.06.11"
toolchain="srs-${version}"

case "$(uname -s)-$(uname -m)" in
    Darwin-arm64)
        target="aarch64-apple-darwin"
        checksum="1af16e9d499b8c0b03598fbc109712f5168aafe102d7e7a182815faa7265c790"
        ;;
    Linux-x86_64)
        target="x86_64-unknown-linux-gnu"
        checksum="cb8381108109251ed491b43ee4473bfdb6f143bf5f17ceea2af98320bb4227a4"
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

cargo_wrapper="$(RUSTUP_TOOLCHAIN="$toolchain" rustup which cargo)"
toolchain_bin="$(dirname "$cargo_wrapper")"
{
    echo "CARGO=${cargo_wrapper}"
    echo "CARGO_INCREMENTAL=0"
    echo "RUSTUP_TOOLCHAIN=${toolchain}"
    echo "SRS_CARGO_ARTIFACT_CACHE_MAX_SIZE=4GiB"
} >> "$GITHUB_ENV"
# Rustup injects the toolchain library tree into LD_LIBRARY_PATH on Linux,
# which disables srs artifact-cache admission due to its nested shared objects.
echo "$toolchain_bin" >> "$GITHUB_PATH"
