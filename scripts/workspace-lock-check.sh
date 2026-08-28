#!/bin/sh

# workspace-lock-check.sh: checks our project lockfile
# plus all workspace script lockfiles.
#
# pass `--write` to update all lockfiles instead of checking.

set -e -o pipefail

if [ "${1-}" = "--write" ]; then
  shift
else
  set -- --check "$@"
fi

# Project lockfile
uv lock "$@"

# All PEP 723 script lockfiles
uv workspace list --scripts | xargs -I {} uv lock --script {} "$@"
