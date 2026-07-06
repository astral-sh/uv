#!/usr/bin/env bash
set -euo pipefail

version="2026.06.25"
toolchain="srs-${version}"

case "$(uname -s)-$(uname -m)" in
    Darwin-arm64)
        target="aarch64-apple-darwin"
        checksum="8d7efde3949d55f65e1da65ed29b06eaf6e3ba3498674152dc9f609ec388de60"
        ;;
    Linux-x86_64)
        target="x86_64-unknown-linux-gnu"
        checksum="b5b04c14fea814166d060742795417a7ab7f9ac87408de4d93d0b202694b48b2"
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
