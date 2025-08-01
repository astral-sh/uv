#!/bin/bash

set -ex

cargo build
uv="$(pwd)/target/debug/uv"
unset VIRTUAL_ENV
export RUST_LOG=uv_distribution_types=debug,uv_distribution::distribution_database=debug

# 1. Single platform
${uv} venv --clear -q -p 3.13 &&  echo "torch" | ${uv} pip compile - --index https://download.pytorch.org/whl/variant --no-annotate --refresh

export NV_VARIANT_PROVIDER_FORCE_CUDA_DRIVER_VERSION=12.8
export NV_VARIANT_PROVIDER_FORCE_SM_ARCH=9.0
${uv} venv --clear -q -p 3.13 && echo "torch" | ${uv} pip compile - --index https://download.pytorch.org/whl/variant --no-annotate
unset NV_VARIANT_PROVIDER_FORCE_CUDA_DRIVER_VERSION
unset NV_VARIANT_PROVIDER_FORCE_SM_ARCH

# 2. Universal with lockfile
# Three cases: Sync (new/updated lock), Sync (fresh lock), Sync (noop)
( cd scripts/packages/torch_user && ${uv} venv -p 3.13 -c -q && rm -f uv.lock && ${uv} sync )
( cd scripts/packages/torch_user && ${uv} venv -p 3.13 -c -q && ${uv} sync )
( cd scripts/packages/torch_user && ${uv} sync )

export NV_VARIANT_PROVIDER_FORCE_CUDA_DRIVER_VERSION=12.8
export NV_VARIANT_PROVIDER_FORCE_SM_ARCH=9.0
( cd scripts/packages/torch_user && ${uv} venv -p 3.13 -c -q && rm -f uv.lock && ${uv} sync )
( cd scripts/packages/torch_user && ${uv} venv -p 3.13 -c -q && ${uv} sync )
( cd scripts/packages/torch_user && ${uv} sync )
unset NV_VARIANT_PROVIDER_FORCE_CUDA_DRIVER_VERSION
unset NV_VARIANT_PROVIDER_FORCE_SM_ARCH

