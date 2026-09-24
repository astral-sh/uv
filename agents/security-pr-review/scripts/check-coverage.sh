#!/usr/bin/env bash
set -euo pipefail

if [ "$(git rev-parse HEAD)" != "$HEAD_SHA" ]; then
  echo "The review changed the checked-out revision." >&2
  exit 1
fi

review="$RUNNER_TEMP/pull-request-review-result.json"
merge_base="$(git merge-base "$BASE_SHA" "$HEAD_SHA")"
git diff --no-ext-diff --no-textconv --no-renames --name-only -z \
  "$merge_base..$HEAD_SHA" | LC_ALL=C sort -z \
  > "$RUNNER_TEMP/expected-review-paths.z"
jq --join-output '.reviewed_paths[] | . + "\u0000"' "$review" \
  | LC_ALL=C sort -z > "$RUNNER_TEMP/reviewed-paths.z"

if ! cmp --silent "$RUNNER_TEMP/expected-review-paths.z" \
  "$RUNNER_TEMP/reviewed-paths.z"; then
  echo "Infrastructure error: the security review did not account for every changed path" >&2
  exit 1
fi
