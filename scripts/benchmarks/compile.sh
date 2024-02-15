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
    "./target/release/axi --no-cache pip-compile ${TARGET} > /tmp/requirements.txt" \
    "./target/release/main --no-cache pip-compile ${TARGET} > /tmp/requirements.txt"

###
# Resolution with a warm cache.
###
hyperfine --runs 20 --warmup 3 --prepare "rm -f /tmp/requirements.txt" \
    "./target/release/axi pip compile ${TARGET} > /tmp/requirements.txt" \
    "./target/release/main pip-compile ${TARGET} > /tmp/requirements.txt"
