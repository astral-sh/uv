#!/usr/bin/env bash

set -euo pipefail

for attempt in {1..5}; do
    pull_requests=$(gh pr list --state open --limit 1000 --json number,mergeable,url,baseRefName,headRefName,headRefOid,isCrossRepository)
    unknown=$(jq '[.[] | select(.mergeable == "UNKNOWN")] | length' <<< "$pull_requests")

    if (( unknown == 0 || attempt == 5 )); then
        break
    fi

    echo "Waiting for GitHub to calculate mergeability for $unknown pull requests (attempt $attempt/5)..."
    sleep 5
done

conflicted=$(jq '[.[] | select(.mergeable == "CONFLICTING")] | length' <<< "$pull_requests")
rebasable=$(jq '[.[] | select(.mergeable == "CONFLICTING" and (.isCrossRepository | not))] | length' <<< "$pull_requests")
cross_repository=$(( conflicted - rebasable ))

if (( rebasable > 256 )); then
    echo "Found $rebasable rebasable pull requests, which exceeds the GitHub Actions matrix limit of 256." >&2
    exit 1
fi

matrix=$(jq --compact-output '{include: [.[] | select(.mergeable == "CONFLICTING" and (.isCrossRepository | not)) | {number, base_ref: .baseRefName, head_ref: .headRefName, head_sha: .headRefOid}]}' <<< "$pull_requests")

{
    echo "matrix=$matrix"
    echo "rebasable=$rebasable"
} >> "$GITHUB_OUTPUT"

{
    echo "## Conflicted pull requests"
    echo
    echo "Found $conflicted conflicted pull requests."

    if (( cross_repository > 0 )); then
        echo
        echo "Skipping $cross_repository cross-repository pull requests that cannot be updated by the uv-dev app token."
    fi

    if (( unknown > 0 )); then
        echo
        echo "GitHub could not determine mergeability for $unknown pull requests."
    fi

    if (( conflicted > 0 )); then
        echo
        jq --raw-output '.[] | select(.mergeable == "CONFLICTING") | "- \(.url)"' <<< "$pull_requests"
    fi
} >> "$GITHUB_STEP_SUMMARY"
