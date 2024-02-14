#!/usr/bin/env bash

# Simple source distribution building integration test using the tqdm (PEP 517) and geoextract (setup.py) sdists.

set -e

mkdir -p sdist_building_test_data/sdist
if [ ! -f sdist_building_test_data/sdist/tqdm-4.66.1.tar.gz ]; then
  wget https://files.pythonhosted.org/packages/62/06/d5604a70d160f6a6ca5fd2ba25597c24abd5c5ca5f437263d177ac242308/tqdm-4.66.1.tar.gz -O sdist_building_test_data/sdist/tqdm-4.66.1.tar.gz
fi
if [ ! -f sdist_building_test_data/sdist/geoextract-0.3.1.tar.gz ]; then
  wget https://files.pythonhosted.org/packages/c4/00/9d9826a6e1c9139cc7183647f47f6b7acb290fa4c572140aa84a12728e60/geoextract-0.3.1.tar.gz -O sdist_building_test_data/sdist/geoextract-0.3.1.tar.gz
fi
rm -rf sdist_building_test_data/wheels
RUST_LOG=uv_build=debug cargo run --bin uv-dev -- build --wheels sdist_building_test_data/wheels sdist_building_test_data/sdist/tqdm-4.66.1.tar.gz
RUST_LOG=uv_build=debug cargo run --bin uv-dev -- build --wheels sdist_building_test_data/wheels sdist_building_test_data/sdist/geoextract-0.3.1.tar.gz

# Check that pip accepts the wheels. It would be better to do functional checks
virtualenv -p 3.8 -q --clear sdist_building_test_data/.venv
sdist_building_test_data/.venv/bin/pip install -q --no-deps sdist_building_test_data/wheels/geoextract-0.3.1-py3-none-any.whl
sdist_building_test_data/.venv/bin/pip install -q --no-deps sdist_building_test_data/wheels/tqdm-4.66.1-py3-none-any.whl
