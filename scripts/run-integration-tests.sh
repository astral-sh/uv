#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR/.."

if command -v python3 > /dev/null; then
  exec python3 "$SCRIPT_DIR/run-integration-tests.py" "$@"
else
  exec python "$SCRIPT_DIR/run-integration-tests.py" "$@"
fi
