#!/bin/bash

# workspace-lock-check.sh: checks our project lockfile
# plus all workspace script lockfiles.
#
# Updates lockfiles by default, run with `--check` to only check.

set -e -o pipefail

# Project lockfile
uv lock "$@"

# All PEP 723 script lockfiles
uv workspace list --scripts | xargs -P4 -I {} uv lock --script {} "$@"
