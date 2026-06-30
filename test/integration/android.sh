#!/bin/sh
# Integration test for bare Android (native x86_64-linux-android uv binary).
# Note: runs under Android's /system/bin/sh (mksh), not bash.
set -eux

export UV_CACHE_DIR="$PWD/.uv-cache"
export UV_PYTHON_INSTALL_DIR="$PWD/.uv-python"

PYTHON_HOME="$PWD/python/prefix"
PYTHON_BIN="$PWD/python/prefix/bin/python"

export PYTHONHOME="$PYTHON_HOME"
export PYTHONPATH="$PYTHON_HOME/lib/python3.14"
export LD_LIBRARY_PATH="$PYTHON_HOME/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"

mkdir -p "$UV_CACHE_DIR" "$UV_PYTHON_INSTALL_DIR"

./uv self version

"$PYTHON_BIN" -V

./uv python list

./uv venv -p "$PYTHON_BIN" .venv

.venv/bin/python -V

./uv pip install -p .venv/bin/python anyio

./uv run -p .venv/bin/python python -c "import anyio; print(anyio.__name__)"

# Exercise a mandatory file lock on Android. Environment locks are treated as
# best-effort in the commands above, but cache cleaning requires an exclusive
# lock, guarding against regressions for https://github.com/rust-lang/rust/issues/148325.
./uv cache clean
