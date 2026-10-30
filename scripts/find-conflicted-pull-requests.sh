#!/usr/bin/env bash

set -euo pipefail

activity_cutoff=$(jq --null-input 'now - 30 * 24 * 60 * 60 | floor')
activity_since=$(jq --null-input --raw-output --argjson cutoff "$activity_cutoff" '$cutoff | todateiso8601')

# GitHub may initially return UNKNOWN while it recalculates mergeability after the base moves.
for attempt in {1..5}; do
    pull_requests=$(gh pr list --base main --state open --limit 1000 "$@" --json number,author,createdAt,mergeable,url,baseRefName,headRefName,headRefOid,headRepository)
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

jq --compact-output '.[] | select(.mergeable == "CONFLICTING")' <<< "$pull_requests" |
    while IFS= read -r pull_request; do
        number=$(jq '.number' <<< "$pull_request")
        active=$(jq --argjson cutoff "$activity_cutoff" '.createdAt | fromdateiso8601 >= $cutoff' <<< "$pull_request")

        if [[ "$active" == false ]]; then
            # Rebases and label changes update the PR timestamp without indicating new work.
            # Inspect the complete timeline so later automation cannot hide older human activity.
            timeline=$(gh api --paginate --slurp "repos/{owner}/{repo}/issues/$number/timeline?per_page=100")
            active=$(
                jq --argjson cutoff "$activity_cutoff" '
                    def bot_commit_identity:
                        (.name // "" | endswith("[bot]")) or
                        (.email // "" | endswith("[bot]@users.noreply.github.com"));
                    [
                        .[][] |
                        if .event == "committed" then
                            # Rebase commits retain their author dates but receive new committer dates.
                            (select(.author | bot_commit_identity | not) | .author.date),
                            (select(.committer | bot_commit_identity | not) | .committer.date)
                        elif (.event == "commented" or .event == "reviewed" or .event == "head_ref_force_pushed")
                            and ((.actor // .user).type == "User") then
                            .created_at, .updated_at, .submitted_at
                        else
                            empty
                        end |
                        select(. != null) | fromdateiso8601
                    ] | any(. >= $cutoff)
                ' <<< "$timeline"
            )
        fi

        if [[ "$active" == false ]]; then
            # Replies to an older review do not necessarily create a new timeline review event.
            comments=$(gh api --method GET --paginate --slurp \
                "repos/{owner}/{repo}/pulls/$number/comments" \
                -f since="$activity_since" -f per_page=100)
            active=$(jq --argjson cutoff "$activity_cutoff" \
                '[.[][] | select(.user.type == "User") | .updated_at | fromdateiso8601] | any(. >= $cutoff)' \
                <<< "$comments")
        fi

        if [[ "$active" == true ]]; then
            echo "$pull_request"
        else
            echo "Skipping pull request #$number: no code or discussion activity in the last 30 days." >&2
        fi
    done |
    jq --slurp --compact-output '[.[] | {number, author: .author.login, url, base_ref: .baseRefName, head_ref: .headRefName, head_sha: .headRefOid, head_repository: .headRepository.nameWithOwner}]'
