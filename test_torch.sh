#!/bin/bash

set -e

uv venv -p 3.13 && cargo run -- pip install torch --index https://download.pytorch.org/whl/test/variant/ --no-progress --no-cache

export NV_VARIANT_PROVIDER_FORCE_KMD_DRIVER_VERSION=525.85.12
export NV_VARIANT_PROVIDER_FORCE_CUDA_DRIVER_VERSION=12.6
uv venv -p 3.13 && cargo run -- pip install torch --index https://download.pytorch.org/whl/test/variant/ --no-progress --index https://variants-index.wheelnext.dev/torch_experiment/ -v --no-cache
