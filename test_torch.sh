#!/bin/bash

set -e

cargo build
unset VIRTUAL_ENV
export RUST_LOG=uv_distribution_types=debug,uv_distribution::distribution_database=debug

# Three cases: Sync (new/updated lock), Sync (fresh lock), Sync (noop)
( cd debug && uv venv -p 3.13 -c -q && rm -f uv.lock && cargo run -q sync )
( cd debug && uv venv -p 3.13 -c -q && cargo run -q sync )
( cd debug && cargo run -q sync )

export NV_VARIANT_PROVIDER_FORCE_CUDA_DRIVER_VERSION=12.8
export NV_VARIANT_PROVIDER_FORCE_SM_ARCH=9.0

( cd debug && uv venv -p 3.13 -c -q && rm -f uv.lock && cargo run -q sync )
( cd debug && uv venv -p 3.13 -c -q && cargo run -q sync )
( cd debug && cargo run -q sync )

unset NV_VARIANT_PROVIDER_FORCE_CUDA_DRIVER_VERSION
unset NV_VARIANT_PROVIDER_FORCE_SM_ARCH

uv venv --clear -q -p 3.13 &&  echo "torch" | cargo run -- pip compile - --index https://variants-index.wheelnext.dev --no-progress --no-annotate # --no-cache

export NV_VARIANT_PROVIDER_FORCE_CUDA_DRIVER_VERSION=12.8
export NV_VARIANT_PROVIDER_FORCE_SM_ARCH=9.0
uv venv --clear -q -p 3.13 && echo "torch" | cargo run -- pip compile - --index https://variants-index.wheelnext.dev --no-annotate # --no-cache

# For user testing
# cargo run pip install torch --index https://download.pytorch.org/whl/test/variant/ --no-cache

