#!/bin/bash

set -e

# No matching variant wheel, no non-variant wheel or sdist
uv venv -q && ( ( UV_CPU_LEVEL_OVERRIDE=0 cargo run pip install built-by-uv --no-index --no-cache --no-progress --find-links ./files && exit 1 ) || exit 0 )
# No matching variant wheel, but a non-variant wheel
uv venv -q && UV_CPU_LEVEL_OVERRIDE=0 cargo run pip install built-by-uv --no-index --no-cache --no-progress --find-links ./files --find-links ./files_wheel
# No matching variant wheel, but a non-variant sdist
uv venv -q && UV_CPU_LEVEL_OVERRIDE=0 cargo run pip install built-by-uv --no-index --no-cache --no-progress --find-links ./files --find-links ./files_sdist
# Matching cpu2 variant wheel
uv venv -q && UV_CPU_LEVEL_OVERRIDE=2 cargo run pip install built-by-uv --no-index --no-cache --no-progress -v --find-links ./files --find-links ./files_wheel
# Matching cpu2 variant wheel, to be preferred over the wheel and the sdist
uv venv -q && UV_CPU_LEVEL_OVERRIDE=2 cargo run pip install built-by-uv --no-index --no-cache --no-progress -v --find-links ./files --find-links ./files_wheel --find-links ./files_sdist
