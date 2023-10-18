#!/usr/bin/env bash

set -e

mkdir -p downloads
if [ ! -f downloads/tqdm-4.66.1.tar.gz ]; then
  wget https://files.pythonhosted.org/packages/62/06/d5604a70d160f6a6ca5fd2ba25597c24abd5c5ca5f437263d177ac242308/tqdm-4.66.1.tar.gz -O downloads/tqdm-4.66.1.tar.gz
fi
if [ ! -f downloads/geoextract-0.3.1.tar.gz ]; then
  wget https://files.pythonhosted.org/packages/c4/00/9d9826a6e1c9139cc7183647f47f6b7acb290fa4c572140aa84a12728e60/geoextract-0.3.1.tar.gz -O downloads/geoextract-0.3.1.tar.gz
fi
rm -rf wheels
RUST_LOG=puffin_build=debug cargo run -p puffin-build --bin puffin-build -- --wheels wheels downloads/tqdm-4.66.1.tar.gz
RUST_LOG=puffin_build=debug cargo run -p puffin-build --bin puffin-build -- --wheels wheels downloads/geoextract-0.3.1.tar.gz

# Check that pip accepts the wheels. It would be better to do functional checks
virtualenv -p 3.8 -q --clear wheels/.venv
wheels/.venv/bin/pip install -q --no-deps wheels/geoextract-0.3.1-py3-none-any.whl
wheels/.venv/bin/pip install -q --no-deps wheels/tqdm-4.66.1-py3-none-any.whl