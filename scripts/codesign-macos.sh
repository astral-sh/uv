#!/usr/bin/env bash
#
# Sign macOS binaries with a code signing identity.
#
# Usage:
#
#   Sign binaries with the given identity:
#
#       $ ./scripts/codesign-macos.sh <identity> <target>...
#
#   For example:
#
#       $ ./scripts/codesign-macos.sh "Mac Developer: Your Name (TEAM_ID)" target/debug/uv
#
#   Use `security find-identity -v -p codesigning` to list available identities.

set -euo pipefail

if [[ "$(uname)" != "Darwin" ]]; then
  echo "Not on macOS, skipping" >&2
  exit 0
fi

if [[ $# -lt 2 ]]; then
  echo "Usage: codesign-macos.sh <identity> <target>..." >&2
  exit 1
fi

identity="$1"
shift

for target in "$@"; do
  if [[ -f "$target" ]]; then
    codesign --force --sign "$identity" "$target"
  fi
done
