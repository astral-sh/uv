#!/usr/bin/env bash
# Download and apply pending insta snapshots from a CI run.
#
# Usage:
#   scripts/apply-ci-snapshots.sh                  # auto-detect PR for current branch
#   scripts/apply-ci-snapshots.sh <run-id>         # use a specific workflow run ID
#   scripts/apply-ci-snapshots.sh <run-id> review  # interactively review instead of accepting
#
# Requires: gh (GitHub CLI), cargo-insta

set -euo pipefail

for cmd in gh cargo-insta git; do
    if ! command -v "$cmd" &>/dev/null; then
        echo "error: '$cmd' is required but not found in PATH" >&2
        exit 1
    fi
done

REPO="astral-sh/uv"
DOWNLOAD_DIR="$(mktemp -d)"

cleanup() {
    rm -rf "$DOWNLOAD_DIR"
}
trap cleanup EXIT

action="${2:-accept}"
if [[ "$action" != "accept" && "$action" != "review" ]]; then
    echo "error: action must be 'accept' or 'review', got '$action'" >&2
    exit 1
fi

# Resolve the run ID
if [[ "${1:-}" ]]; then
    run_id="$1"
else
    # Auto-detect: find the latest CI run for the current branch's PR
    branch="$(git branch --show-current)"
    if [[ -z "$branch" ]]; then
        echo "error: not on a branch and no run ID provided" >&2
        exit 1
    fi

    pr_number="$(gh pr view "$branch" --repo "$REPO" --json number --jq '.number' 2>/dev/null || true)"
    if [[ -z "$pr_number" ]]; then
        echo "error: no PR found for branch '$branch'" >&2
        exit 1
    fi

    echo "Found pull request #$pr_number for branch '$branch'..."

    run_id="$(gh run list \
        --repo "$REPO" \
        --workflow ci.yml \
        --branch "$branch" \
        --limit 1 \
        --json databaseId \
        --jq '.[0].databaseId')"
    if [[ -z "$run_id" ]]; then
        echo "error: no CI runs found for branch '$branch'" >&2
        exit 1
    fi
    echo "Found latest CI run $run_id"
fi

# Download all pending-snapshots artifacts from the run
echo "Downloading pending snapshot artifacts..."
mkdir -p "$DOWNLOAD_DIR"
gh run download "$run_id" \
    --repo "$REPO" \
    --pattern "pending-snapshots-*" \
    --dir "$DOWNLOAD_DIR" 2>/dev/null || true

# Check if any artifacts were downloaded
artifact_count="$(find "$DOWNLOAD_DIR" -mindepth 1 -maxdepth 1 -type d | wc -l | tr -d ' ')"
if [[ "$artifact_count" -eq 0 ]]; then
    echo "No pending snapshot artifacts found in run $run_id."
    echo "Either the tests passed or no snapshot mismatches occurred."
    exit 0
fi

s=$( (( artifact_count != 1 )) && echo "s" || true)
echo "Downloaded $artifact_count artifact$s"

# Merge all artifacts into a single pending-snapshots directory.
# Different platforms may produce different snapshots; we collect them all.
merged_dir="$DOWNLOAD_DIR/_merged"
mkdir -p "$merged_dir"
for artifact_dir in "$DOWNLOAD_DIR"/pending-snapshots-*/; do
    [[ -d "$artifact_dir" ]] || continue
    cp -rn "$artifact_dir"/* "$merged_dir"/ 2>/dev/null || true
done

if ! find "$merged_dir" -type f \( -name '*.snap.new' -o -name '*.pending-snap' \) | grep -q .; then
    echo "No pending snapshot files found in the artifacts."
    exit 0
fi

echo "Applying snapshot changes..."

# Use cargo-insta with INSTA_PENDING_DIR to apply the snapshots
INSTA_PENDING_DIR="$merged_dir" cargo insta "$action" --workspace
