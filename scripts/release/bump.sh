#!/usr/bin/env bash
# Prepare for a release
#
# All additional options are passed to `rooster`
set -eu

script_root="$(realpath "$(dirname "$0")")"
project_root="$(dirname "$(dirname "$script_root")")"

cd "$script_root"
echo "Setting up a temporary environment..."
uv venv

source ".venv/bin/activate"
uv pip install -r requirements.txt

echo "Updating metadata with rooster..."
cd "$project_root"
rooster release "$@"

echo "Updating lockfile..."
cargo check

echo "Generating contributors list..."
echo ""
echo ""
rooster contributors --quiet
