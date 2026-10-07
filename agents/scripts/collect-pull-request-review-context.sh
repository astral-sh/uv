#!/usr/bin/env bash
set -euo pipefail

: "${REVIEW_CONFIG:?Review configuration directory is required}"
: "${PULL_REQUEST_NUMBER:?Pull request number is required}"
: "${BASE_SHA:?Pull request base SHA is required}"
: "${HEAD_SHA:?Pull request head SHA is required}"
: "${GITHUB_REPOSITORY:?Repository is required}"

if [ "$(git rev-parse HEAD)" != "$HEAD_SHA" ]; then
  echo "The checkout does not match the pull request head." >&2
  exit 1
fi

context="$REVIEW_CONFIG/context"
mkdir -p "$context"
gh pr view "$PULL_REQUEST_NUMBER" \
  --repo "$GITHUB_REPOSITORY" \
  --json number,title,body,author,baseRefName,baseRefOid,headRefName,headRefOid,isDraft,labels \
  > "$context/event.json"

if ! jq --exit-status --arg head "$HEAD_SHA" \
  '.headRefOid == $head' "$context/event.json" > /dev/null; then
  echo "The pull request head changed during review setup." >&2
  exit 1
fi

merge_base="$(git merge-base "$BASE_SHA" "$HEAD_SHA")"
jq --null-input --arg base "$merge_base" --arg base_tip "$BASE_SHA" --arg head "$HEAD_SHA" \
  '{base: $base, base_tip: $base_tip, head: $head}' > "$context/revisions.json"
git diff --no-ext-diff --no-textconv --no-renames \
  "$merge_base..$HEAD_SHA" > "$context/diff.patch"
git diff --no-ext-diff --no-textconv --no-renames --name-only -z \
  "$merge_base..$HEAD_SHA" \
  | jq --raw-input --slurp 'split("\u0000")[:-1]' > "$context/paths.json"

gh api --paginate --slurp \
  "repos/$GITHUB_REPOSITORY/pulls/$PULL_REQUEST_NUMBER/comments?per_page=100" \
  | jq '[.[][] | {author: .user.login, body, path, line, original_line, side, start_line,
      original_start_line, start_side, commit_id, original_commit_id, in_reply_to_id,
      created_at, html_url}]' > "$context/comments.json"
gh api --paginate --slurp \
  "repos/$GITHUB_REPOSITORY/pulls/$PULL_REQUEST_NUMBER/reviews?per_page=100" \
  | jq '[.[][] | {author: .user.login, body, state, commit_id, submitted_at, html_url}]' \
    > "$context/reviews.json"
gh api --paginate --slurp \
  "repos/$GITHUB_REPOSITORY/issues/$PULL_REQUEST_NUMBER/comments?per_page=100" \
  | jq '[.[][] | {author: .user.login, body, created_at, html_url}]' \
    > "$context/conversation.json"
