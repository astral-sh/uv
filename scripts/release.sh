#!/usr/bin/env bash
# Prepare for a release
#
# All additional options are passed to `rooster`
set -eu

script_root="$(realpath "$(dirname "$0")")"
project_root="$(dirname "$script_root")"

echo "Updating metadata with rooster..."
cd "$project_root"

# Update the changelog
uv tool run --from 'rooster-blue>=0.0.7' --python 3.12 -- \
    rooster release "$@"

echo "Updating lockfile..."
cargo update -p uv
