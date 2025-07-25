#!/bin/bash

set -e

cargo build
uv="$(pwd)/target/debug/uv"
unset VIRTUAL_ENV
export RUST_LOG=uv_distribution_types=debug,uv_distribution::distribution_database=debug

echo "# No matching variant wheel, no non-variant wheel or sdist"
uv venv -c -q && ( ( UV_CPU_LEVEL_OVERRIDE=0 ${uv} pip install built-by-uv --no-index --no-cache --no-progress --find-links ./files && exit 1 ) || exit 0 )
echo "# No matching variant wheel, but a non-variant wheel"
uv venv -c -q && UV_CPU_LEVEL_OVERRIDE=0 ${uv} pip install built-by-uv --no-index --no-cache --no-progress --find-links ./files --find-links ./files_wheel
echo "# No matching variant wheel, but a non-variant sdist"
uv venv -c -q && UV_CPU_LEVEL_OVERRIDE=0 ${uv} pip install built-by-uv --no-index --no-cache --no-progress --find-links ./files --find-links ./files_sdist
echo "# Matching cpu2 variant wheel, to be preferred over the non-variant wheel"
uv venv -c -q && UV_CPU_LEVEL_OVERRIDE=2 ${uv} pip install built-by-uv --no-index --no-cache --no-progress --find-links ./files --find-links ./files_wheel
echo "# Matching cpu2 variant wheel, to be preferred over the non-variant wheel and the sdist"
uv venv -c -q && UV_CPU_LEVEL_OVERRIDE=2 RUST_LOG=uv_distribution_types=debug ${uv} pip install built-by-uv --no-index --no-cache --no-progress --find-links ./files --find-links ./files_wheel --find-links ./files_sdist

echo "# sync without a compatible variant wheel (fresh)"
( cd scripts/packages/cpu_user && rm -f uv.lock && ( ( uv venv -c -q && UV_CPU_LEVEL_OVERRIDE=0 ${uv} sync && exit 1 ) || exit 0 ) )
echo "# sync without a compatible variant wheel (existing lockfile)"
( cd scripts/packages/cpu_user && rm -f uv.lock && ( ( uv venv -c -q && UV_CPU_LEVEL_OVERRIDE=0 ${uv} sync && exit 1 ) || exit 0 ) )
echo "# sync with a compatible variant wheel"
# TODO(konsti): Selecting a different level currently selects the right wheel, but doesn't reinstall.
( cd scripts/packages/cpu_user && rm -f uv.lock && uv venv -c -q && UV_CPU_LEVEL_OVERRIDE=2 ${uv} sync && UV_CPU_LEVEL_OVERRIDE=2 ${uv} sync && UV_CPU_LEVEL_OVERRIDE=3 ${uv} sync )
