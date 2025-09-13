#!/usr/bin/env bash
#
# Sync test scenarios with the pinned version of packse.
#
# Usage:
#
#   Install the pinned packse version in a temporary virtual environment, fetch scenarios, and regenerate test cases and snapshots:
#
#       $ ./scripts/sync_scenarios.sh
#
#   Additional arguments are passed to `./scripts/scenarios/generate.py`, for example:
#
#       $ ./scripts/sync_scenarios.sh --verbose --no-snapshot-update
#
#   For development purposes, the `./scripts/scenarios/generate.py` script can be used directly to generate
#   test cases from a local set of scenarios.
#
#   To update the packse version, run the following command first:
#
#       $ uv pip compile --group scripts/scenarios/pyproject.toml:packse -o scripts/scenarios/pylock.toml --upgrade-package packse
#
# See `scripts/scenarios/` for supporting files.
set -eu

script_root="$(realpath "$(dirname "$0")")"


cd "$script_root/scenarios"
echo "Setting up a temporary environment..."
uv venv -p 3.12 -c

# shellcheck disable=SC1091
source ".venv/bin/activate"
uv pip install -r "$script_root/scenarios/pylock.toml" --refresh-package packse

echo "Fetching packse scenarios..."
packse fetch --dest "$script_root/scenarios/.downloads" --force

unset VIRTUAL_ENV # Avoid warning due to venv mismatch
.venv/bin/python "$script_root/scenarios/generate.py" "$script_root/scenarios/.downloads" "$@"

# Cleanup
rm -r "$script_root/scenarios/.downloads"
