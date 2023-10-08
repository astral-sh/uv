#!/usr/bin/env sh

###
# Benchmark the resolver against `pip-compile`.
#
# Example usage:
#
#   ./scripts/benchmarks/compile.sh ./scripts/benchmarks/requirements.in
###

set -euxo pipefail

TARGET=${1}

###
# Resolution with a cold cache.
###
hyperfine --runs 20 --warmup 3 --prepare "rm -f /tmp/requirements.txt" \
    "./target/release/puffin-cli compile ${TARGET} --no-cache > /tmp/requirements.txt" \
    "pip-compile ${TARGET} --rebuild --pip-args '--no-cache-dir' -o /tmp/requirements.txt"

###
# Resolution with a warm cache.
###
hyperfine --runs 20 --warmup 3 --prepare "rm -f /tmp/requirements.txt" \
    "./target/release/puffin-cli compile ${TARGET} > /tmp/requirements.txt" \
    "pip-compile ${TARGET} -o /tmp/requirements.txt"
