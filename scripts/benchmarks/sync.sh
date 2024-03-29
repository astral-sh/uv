#!/usr/bin/env bash

###
# Benchmark the installer against `pip`.
#
# Example usage:
#
#   ./scripts/benchmarks/sync.sh ./scripts/benchmarks/requirements.txt
###

set -euxo pipefail

TARGET=${1}

###
# Installation with a cold cache.
###
hyperfine --runs 20 --warmup 3 \
    --prepare "virtualenv --clear .venv" \
    "./target/release/uv pip sync ${TARGET} --no-cache" \
    --prepare "rm -rf /tmp/site-packages" \
    "pip install -r ${TARGET} --target /tmp/site-packages --no-cache-dir --no-deps"

###
# Installation with a warm cache, similar to blowing away and re-creating a virtual environment.
###
hyperfine --runs 20 --warmup 3 \
    --prepare "virtualenv --clear .venv" \
    "./target/release/uv pip sync ${TARGET}" \
    --prepare "rm -rf /tmp/site-packages" \
    "pip install -r ${TARGET} --target /tmp/site-packages --no-deps"

###
# Installation with all dependencies already installed (no-op).
###
hyperfine --runs 20 --warmup 3 \
    --setup "virtualenv --clear .venv && source .venv/bin/activate" \
    "./target/release/uv pip sync ${TARGET}" \
    "pip install -r ${TARGET} --no-deps"
