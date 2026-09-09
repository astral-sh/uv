#!/usr/bin/env bash
# Print the files published in a cargo-dist GitHub release, one path per line.
# The manifest and its release assets must be in the same directory.
# Requires `jq`.

set -euo pipefail

manifest=${1:?path to dist-manifest.json is required}
directory=$(dirname -- "$manifest")

# cargo-dist does not include the manifest itself in its release asset list.
printf '%s\n' "$manifest"
jq -r --arg directory "$directory" \
    '.releases[].artifacts[] | "\($directory)/\(.)"' "$manifest"
