#!/bin/bash

set -e

cargo build
unset VIRTUAL_ENV
export RUST_LOG=uv_distribution_types=debug,uv_distribution::distribution_database=debug

# No matching variant wheel, no non-variant wheel or sdist
uv venv -c -q && ( ( UV_CPU_LEVEL_OVERRIDE=0 cargo run -q pip install built-by-uv --no-index --no-cache --no-progress --find-links ./files && exit 1 ) || exit 0 )
# No matching variant wheel, but a non-variant wheel
uv venv -c -q && UV_CPU_LEVEL_OVERRIDE=0 cargo run -q pip install built-by-uv --no-index --no-cache --no-progress --find-links ./files --find-links ./files_wheel
# No matching variant wheel, but a non-variant sdist
uv venv -c -q && UV_CPU_LEVEL_OVERRIDE=0 cargo run -q pip install built-by-uv --no-index --no-cache --no-progress --find-links ./files --find-links ./files_sdist
# Matching cpu2 variant wheel, to be preferred over the non-variant wheel
uv venv -c -q && UV_CPU_LEVEL_OVERRIDE=2 cargo run -q pip install built-by-uv --no-index --no-cache --no-progress --find-links ./files --find-links ./files_wheel
# Matching cpu2 variant wheel, to be preferred over the non-variant wheel and the sdist
uv venv -c -q && UV_CPU_LEVEL_OVERRIDE=2 RUST_LOG=uv_distribution_types=debug cargo run -q pip install built-by-uv --no-index --no-cache --no-progress --find-links ./files --find-links ./files_wheel --find-links ./files_sdist

# sync without a compatible variant wheel
( cd scripts/packages/cpu_user && rm -f uv.lock && ( ( uv venv -c -q && UV_CPU_LEVEL_OVERRIDE=0 cargo run -q sync && exit 1 ) || exit 0 ) && ( ( uv venv -c -q && UV_CPU_LEVEL_OVERRIDE=0 cargo run -q sync && exit 1 ) || exit 0 ) )
# sync with a compatible variant wheel
# TODO(konsti): Selecting a different level currently selects the right wheel, but doesn't reinstall.
( cd scripts/packages/cpu_user && rm -f uv.lock && uv venv -c -q && UV_CPU_LEVEL_OVERRIDE=2 cargo run -q sync && UV_CPU_LEVEL_OVERRIDE=2 cargo run -q sync && UV_CPU_LEVEL_OVERRIDE=3 cargo run -q sync )
