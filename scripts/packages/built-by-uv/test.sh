#!/bin/bash
# Test source tree -> source dist -> wheel and run pytest after.
# We can't test this through the build backend setting directly since we want
# to use the debug build of uv, so we use the internal API instead.

set -e

cargo build
uv venv -p 3.12 -q
mkdir -p dist
rm -f dist/*
../../../target/debug/uv build-backend build-sdist dist/
rm -rf build-root
mkdir build-root
cd build-root
tar -tvf ../dist/built_by_uv-0.1.0.tar.gz
tar xf ../dist/built_by_uv-0.1.0.tar.gz
cd built-by-uv-0.1.0
../../../../../target/debug/uv build-backend build-wheel ../../dist
unzip -l ../../dist/built_by_uv-0.1.0-py3-none-any.whl
cd ../..
uv pip install -q pytest dist/built_by_uv-0.1.0-py3-none-any.whl
pytest
