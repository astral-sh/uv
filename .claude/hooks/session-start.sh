#!/bin/bash
set -euo pipefail

# Read hook input from stdin (JSON with session info)
INPUT=$(cat)
SOURCE=$(echo "$INPUT" | jq -r '.source // "startup"')

# Skip on resume - tools are already installed from the initial session
if [ "$SOURCE" = "resume" ]; then
  exit 0
fi

# Dispatch to web hook if running remotely
if [ "${CLAUDE_CODE_REMOTE:-}" = "true" ]; then
  exec "$(dirname "$0")/session-start-web.sh"
fi
