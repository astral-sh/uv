#!/usr/bin/env bash
# Prepare for a release
#
# All additional options are passed to `rooster`
set -eu

script_root="$(realpath "$(dirname "$0")")"
project_root="$(dirname "$script_root")"

echo "Updating metadata with rooster..."
cd "$project_root"

# Update the preview changelog
uv tool run --from 'rooster-blue>=0.0.7' --python 3.12 -- \
    rooster release "$@" \
    --only-sections preview \
    --changelog-file PREVIEW-CHANGELOG.md \
    --no-update-pyproject --no-update-version-files

# Update the real changelog
uv tool run --from 'rooster-blue>=0.0.7' --python 3.12 -- \
    rooster release "$@" --without-sections preview

echo "Updating lockfile..."
cargo update -p uv
