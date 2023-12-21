#!/bin/bash
# @konstin's script for providing feedback for pubgrub changes

# No `set -e`, these are error cases

# shellcheck disable=SC2164
cd "$(git rev-parse --show-toplevel)"

git checkout Cargo.toml
echo "main: $(cat Cargo.toml | grep pubgrub | sed -e 's/.*rev = "\(.*\)".*/\1/g')"
echo "branch: $(cd ../pubgrub && git rev-parse HEAD)"

cargo build --profile profiling
cp target/profiling/puffin target/profiling/main
cp target/profiling/puffin-dev target/profiling/main-dev

# Patch Cargo.toml
sed -i 's/pubgrub = .*/pubgrub = { path = "..\/pubgrub" }/g' Cargo.toml

cargo build --profile profiling

mkdir -p target/logs

virtualenv -p 3.12 --clear -q .venv312
echo "tf-models-nightly main"
time VIRTUAL_ENV=.venv312 target/profiling/main pip-compile scripts/requirements/tf-models-nightly.txt 2> target/logs/tf-models-nightly-main.txt
echo "tf-models-nightly branch"
time VIRTUAL_ENV=.venv312 target/profiling/puffin pip-compile scripts/requirements/tf-models-nightly.txt 2> target/logs/tf-models-nightly-branch.txt

# I don't understand why this doesn't work, the flamegraphs either never finish (https://michcioperz.com/wiki/slow-perf-script/)
# or they are unusable compared to sample record, which doesn't seem to have this problem at all but can't export svg flamegraphs
# https://github.com/flamegraph-rs/flamegraph/pull/127
# VIRTUAL_ENV=.venv312 perf record --call-graph dwarf target/profiling/main pip-compile scripts/requirements/tf-models-nightly.txt 2> /dev/null
# flamegraph --perfdata perf.data --no-inline -o flamegraph-tf-models-nightly-main.svg
# VIRTUAL_ENV=.venv312 perf record --call-graph dwarf target/profiling/branch pip-compile scripts/requirements/tf-models-nightly.txt 2> /dev/null
# flamegraph --perfdata perf.data --no-inline -o flamegraph-tf-models-nightly-branch.svg --open

virtualenv -p 3.10 --clear -q .venv310
echo "bio_embeddings main"
time VIRTUAL_ENV=.venv310 target/profiling/main pip-compile scripts/requirements/bio_embeddings.txt 2> target/logs/bio_embeddings-main.txt > /dev/null
echo "bio_embeddings branch"
time VIRTUAL_ENV=.venv310 target/profiling/puffin pip-compile scripts/requirements/bio_embeddings.txt 2> target/logs/bio_embeddings-branch.txt > /dev/null

git checkout Cargo.toml
