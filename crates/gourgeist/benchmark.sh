#!/usr/bin/env bash

set -e

cd "$(git rev-parse --show-toplevel)"

virtualenv --version

cargo build --profile profiling --bin gourgeist --features cli

hyperfine --warmup 1 --shell none --prepare "rm -rf target/venv-benchmark" \
  "target/profiling/gourgeist -p 3.11 target/venv-benchmark" \
  "virtualenv -p 3.11 --no-seed target/venv-benchmark"

