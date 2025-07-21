#!/bin/bash

set -e

uv venv --clear -q -p 3.13 &&  echo "torch" | cargo run -q -- pip compile - --index https://download.pytorch.org/whl/test/variant/ --no-progress --no-annotate -v # --no-cache

export NV_VARIANT_PROVIDER_FORCE_KMD_DRIVER_VERSION=570.133.20
export NV_VARIANT_PROVIDER_FORCE_CUDA_DRIVER_VERSION=12.8
uv venv --clear -q -p 3.13 && echo "torch" | cargo run -q -- pip compile - --index https://download.pytorch.org/whl/test/variant/ --no-progress --no-annotate -v # --no-cache

# For user testing
# cargo run pip install torch --index https://download.pytorch.org/whl/test/variant/ --no-cache
