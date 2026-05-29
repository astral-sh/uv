#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

if command -v python3 > /dev/null; then
  exec python3 "$SCRIPT_DIR/nextest-setup-build.py"
else
  exec python "$SCRIPT_DIR/nextest-setup-build.py"
fi
