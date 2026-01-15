#!/bin/bash
set -euo pipefail

# Dispatch to web hook if running remotely
if [ "${CLAUDE_CODE_REMOTE:-}" = "true" ]; then
  exec "$(dirname "$0")/session-start-web.sh"
fi
