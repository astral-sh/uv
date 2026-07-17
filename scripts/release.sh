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
uv run --locked --python 3.12 --only-group release rooster release "$@"

# Bump library crate versions
uv run "$project_root/scripts/bump-workspace-crate-versions.py"

echo "Updating crate READMEs..."
uv run "$project_root/scripts/generate-crate-readmes.py"

echo "Updating lockfiles..."
cargo update -p uv
pushd crates/uv-trampoline; cargo update -p uv-trampoline; popd
uv lock --no-config

echo "Generating JSON schema..."
cargo dev generate-json-schema

echo "Checking crates.io publish setup..."
crates_policies="$(mktemp -d)"
trap 'rm -rf "$crates_policies"' EXIT
git clone --depth=1 --quiet https://github.com/astral-sh/crates-policies.git "$crates_policies"
uv run --no-config "$crates_policies/check.py" "$project_root"

echo "Creating release branch..."
git checkout -b "release/$(uv version --short)"
git commit -am "Bump version to $(uv version --short)"
