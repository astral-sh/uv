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
uv tool run --from 'rooster-blue @ git+https://github.com/zanieb/rooster@c24ea11bf3cfea89d6f8c782462cac4313e5e0d6' --python 3.12 -- \
    rooster release "$@"

echo "Updating lockfile..."
cargo update -p uv
