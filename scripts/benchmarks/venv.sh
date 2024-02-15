#!/usr/bin/env bash

###
# Benchmark the virtualenv initialization against `virtualenv`.
#
# Example usage:
#
#   ./scripts/benchmarks/venv.sh
###

set -euxo pipefail

###
# Create a virtual environment without seed packages.
###
hyperfine --runs 20 --warmup 3 \
    --prepare "rm -rf .venv" \
    "./target/release/axi venv" \
    --prepare "rm -rf .venv" \
    "virtualenv --without-pip .venv" \
    --prepare "rm -rf .venv" \
    "python -m venv --without-pip .venv"

###
# Create a virtual environment with seed packages.
###
hyperfine --runs 20 --warmup 3 \
    --prepare "rm -rf .venv" \
    "./target/release/axi venv --seed" \
    --prepare "rm -rf .venv" \
    "virtualenv .venv" \
    --prepare "rm -rf .venv" \
    "python -m venv .venv"
