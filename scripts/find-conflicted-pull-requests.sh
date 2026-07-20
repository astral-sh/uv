#!/usr/bin/env bash

set -euo pipefail

arguments=()
if [[ "${GH_REPO:-}" == "astral-sh/uv" ]]; then
    arguments+=(--author app/astral-automations-bot)
fi

# GitHub may initially return UNKNOWN while it recalculates mergeability after the base moves.
for attempt in {1..5}; do
    pull_requests=$(gh pr list --base main --state open --limit 1000 "${arguments[@]}" --json number,author,mergeable,url,baseRefName,headRefName,headRefOid,headRepository)
    unknown=$(jq '[.[] | select(.mergeable == "UNKNOWN")] | length' <<< "$pull_requests")

    if (( unknown == 0 || attempt == 5 )); then
        break
    fi

    echo "Waiting for GitHub to calculate mergeability for $unknown pull requests (attempt $attempt/5)..." >&2
    sleep 5
done

if (( unknown > 0 )); then
    echo "GitHub could not determine mergeability for $unknown pull requests." >&2
fi

jq --compact-output '[.[] | select(.mergeable == "CONFLICTING") | {number, author: .author.login, url, base_ref: .baseRefName, head_ref: .headRefName, head_sha: .headRefOid, head_repository: .headRepository.nameWithOwner}]' <<< "$pull_requests"
