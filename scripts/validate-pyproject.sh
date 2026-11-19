#!/usr/bin/env bash

set -euo pipefail

cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."

# Replace the bundled `uv` schema with the checked-in schema. `validate-pyproject` requires
# an absolute `$id` to resolve the schema's internal references.
# shellcheck disable=SC2016 # Keep the jaq filter literal.
uv run --only-group=check jaq '.["$id"]="https://example.com/uv.schema.json"' uv.schema.json |
  uv run --only-group=check validate-pyproject --disable-plugins uv --tool uv=/dev/stdin pyproject.toml
