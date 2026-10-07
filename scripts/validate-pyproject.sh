#!/usr/bin/env bash

set -euo pipefail

cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."

# Replace the bundled `uv` schema with the checked-in schema. `validate-pyproject` requires
# an absolute `$id` to resolve the schema's internal references.
uv run --only-group=check validate-pyproject \
  --disable-plugins uv \
  --tool uv=<(jq '.["$id"]="https://example.com/uv.schema.json"' uv.schema.json) \
  pyproject.toml
