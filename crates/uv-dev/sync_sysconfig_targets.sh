#!/usr/bin/env bash
set -euo pipefail

# Fetch latest python-build-standalone tag
latest_tag=$(curl -fsSL -H "Accept: application/json" https://github.com/astral-sh/python-build-standalone/releases/latest | jq -r .tag_name)

# Validate we got a tag name back
if [[ -z "${latest_tag}" ]]; then
  echo "Error: Failed to fetch the latest tag from astral-sh/python-build-standalone." >&2
  exit 1
fi

# Edit the sysconfig mapping endpoints
sed -i.bak -E "s|refs/tags/[^/]+/cpython-unix|refs/tags/${latest_tag}/cpython-unix|g" src/generate_sysconfig_mappings.rs && rm -f src/generate_sysconfig_mappings.rs.bak
sed -i.bak -E "s|blob/[^/]+/cpython-unix|blob/${latest_tag}/cpython-unix|g" src/generate_sysconfig_mappings.rs && rm -f src/generate_sysconfig_mappings.rs.bak

# Regenerate mappings in case there's any changes
cargo dev generate-sysconfig-metadata
