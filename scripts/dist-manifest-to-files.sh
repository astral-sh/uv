#!/usr/bin/env bash
# Print the files published in a cargo-dist GitHub release, one path per line.
# The manifest and its release assets must be in the same directory.
# Requires `jaq`.

set -euo pipefail

manifest=${1:?path to dist-manifest.json is required}
directory=$(dirname -- "$manifest")

# cargo-dist does not include the manifest itself in its release asset list.
printf '%s\n' "$manifest"
# shellcheck disable=SC2016 # Keep the jaq filter literal.
jaq -r --arg directory "$directory" \
    '.releases[].artifacts[] | "\($directory)/\(.)"' "$manifest"
