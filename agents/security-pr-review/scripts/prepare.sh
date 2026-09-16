#!/usr/bin/env bash
set -euo pipefail

script_directory="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
delimiter="review-$(openssl rand -hex 16)"
{
  printf 'result<<%s\n' "$delimiter"
  printf '%s\n' "$REVIEW_RESULT" \
    | uv run --locked --script "$script_directory/agent-review-to-github-comments.py" --commit-id "$HEAD_SHA"
  printf '%s\n' "$delimiter"
} >> "$GITHUB_OUTPUT"
