#!/usr/bin/env bash

set -e

virtualenv --version

#cargo build --profile profiling
cargo build --release #--features parallel
# Benchmarking trick! strip your binaries ٩( ∂‿∂ )۶
strip target/release/gourgeist

echo "## Bare"
hyperfine --warmup 1 --prepare "rm -rf target/a" "virtualenv -p 3.11 --no-seed target/a" "target/release/gourgeist -p 3.11 --bare target/a"
echo "## Default"
hyperfine --warmup 1 --prepare "rm -rf target/a" "virtualenv -p 3.11 target/a" "target/release/gourgeist -p 3.11 target/a"

