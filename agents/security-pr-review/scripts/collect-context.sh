#!/usr/bin/env bash
set -euo pipefail

if [ "$(git rev-parse HEAD)" != "$HEAD_SHA" ]; then
  echo "The checkout does not match the pull request head." >&2
  exit 1
fi

gh pr view "$PULL_REQUEST_NUMBER" \
  --repo "$GITHUB_REPOSITORY" \
  --json number,title,body,author,baseRefName,baseRefOid,headRefName,headRefOid,isDraft,labels,files,additions,deletions,changedFiles \
  > .pull-request-review-event.json

if ! jq --exit-status --arg head "$HEAD_SHA" \
  '.headRefOid == $head' .pull-request-review-event.json > /dev/null; then
  echo "The pull request head changed during review setup." >&2
  exit 1
fi

merge_base="$(git merge-base "$BASE_SHA" "$HEAD_SHA")"
jq --null-input --arg base "$merge_base" --arg base_tip "$BASE_SHA" \
  --arg head "$HEAD_SHA" \
  '{base: $base, base_tip: $base_tip, head: $head}' \
  > .pull-request-review-revisions.json

git diff --no-ext-diff --no-textconv --no-renames \
  "$merge_base..$HEAD_SHA" > .pull-request-review.diff
git diff --no-ext-diff --no-textconv --no-renames --name-only -z \
  "$merge_base..$HEAD_SHA" > "$RUNNER_TEMP/pull-request-review-paths.z"

: > .pull-request-review-paths.txt
while IFS= read -r -d '' path; do
  if [[ "$path" == *$'\n'* || "$path" == *$'\r'* ]]; then
    echo "A changed path cannot be represented in the scan inventory." >&2
    exit 1
  fi
  printf '%s\n' "$path" >> .pull-request-review-paths.txt
done < "$RUNNER_TEMP/pull-request-review-paths.z"

gh api --paginate --slurp \
  "repos/$GITHUB_REPOSITORY/pulls/$PULL_REQUEST_NUMBER/comments?per_page=100" \
  | jq '[.[][] | {author: .user.login, body, path, line, original_line, side, start_line, original_start_line, start_side, commit_id, original_commit_id, in_reply_to_id, created_at, html_url}]' \
  > .pull-request-review-comments.json

{
  printf 'Pull request security review for #%s: %s\n\n' \
    "$(jq -r '.number' .pull-request-review-event.json)" \
    "$(jq -r '.title | gsub("[\r\n]+"; " ")' .pull-request-review-event.json)"
  cat "$REVIEW_CONFIG/prompt.md"
} > "$RUNNER_TEMP/pull-request-security-review-prompt.md"
