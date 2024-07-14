#!/bin/bash

set -e

cd "$(git rev-parse --show-toplevel)"

RUSTFLAGS=-Zprint-type-sizes cargo +nightly build -p uv -j 1 > scripts/stack-overflow/type-sizes.txt
top-type-sizes -w -s -h 10 < scripts/stack-overflow/type-sizes.txt > scripts/stack-overflow/sizes.txt
