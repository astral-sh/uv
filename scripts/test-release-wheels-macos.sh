#!/usr/bin/env bash
# Install and run the macOS wheels in the given directory.
set -euo pipefail

wheel_directory=$1
virtual_environment=$(mktemp -d "${TMPDIR:-/tmp}/uv-release-wheels.XXXXXX")
trap 'rm -rf "$virtual_environment"' EXIT

python3 -m venv "$virtual_environment"
python=("$virtual_environment/bin/python")
# The Intel wheels are verified on an ARM runner; pip needs the Intel Python slice.
if [[ "$wheel_directory" == *x86_64-apple-darwin ]]; then
  python=(arch -x86_64 "$virtual_environment/bin/python")
fi
"${python[@]}" -m pip install --no-index --no-deps "$wheel_directory"/*.whl
for binary in uv uvx uv-build; do
  "$virtual_environment/bin/$binary" --help
done
