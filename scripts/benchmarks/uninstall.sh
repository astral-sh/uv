#!/usr/bin/env sh

###
# Benchmark the uninstall command against `pip`.
#
# Example usage:
#
#   ./scripts/benchmarks/uninstall.sh numpy
###

set -euxo pipefail

TARGET=${1}

hyperfine --runs 20 --warmup 3 --prepare "rm -rf .venv && virtualenv .venv && source activate .venv/bin/activate && pip install ${TARGET}" \
    "./target/release/puffin uninstall ${TARGET}" \
    "pip uninstall -y ${TARGET}"
