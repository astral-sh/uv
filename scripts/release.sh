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
uvx --python 3.12 rooster@0.1.1 release "$@"

# Bump library crate versions
uv run "$project_root/scripts/bump-workspace-crate-versions.py"

echo "Updating crate READMEs..."
uv run "$project_root/scripts/generate-crate-readmes.py"

echo "Updating lockfile..."
cargo update -p uv
pushd crates/uv-trampoline; cargo update -p uv-trampoline; popd

echo "Generating JSON schema..."
cargo dev generate-json-schema
