#!/usr/bin/env bash
#
# Manually perform a GitHub release for a broken automated release that otherwise went out.
#
# PLEASE USE WITH CAUTION.
#
# It can take a while to download all the artifacts.
#
# Requires the `gh` CLI.

set -euo pipefail

if [ -z "${COMMIT:-}" ]; then
    echo "COMMIT is required."
    exit 1
fi

if [ -z "${RUN_ID:-}" ]; then
    echo "RUN_ID is required."
    exit 1
fi

# Create directory for artifacts
mkdir -p "release_$RUN_ID"
cd "release_$RUN_ID"

REPO=$(gh repo view --json nameWithOwner | jq .nameWithOwner -r)

# Download publication artifacts and signed archives from the same run attempt.
RUN_ATTEMPT=$(gh run view "$RUN_ID" --repo "$REPO" --json attempt --jq .attempt)
SIGNED_ARCHIVES="signed-github-archives-$RUN_ID-$RUN_ATTEMPT"
gh run download "$RUN_ID" --repo "$REPO" --pattern 'release-github-*'
gh run download "$RUN_ID" --repo "$REPO" --pattern 'build-github-archives-*-linux-*'
gh run download "$RUN_ID" --repo "$REPO" --name "$SIGNED_ARCHIVES" --dir "$SIGNED_ARCHIVES"

MANIFEST="release-github-manifest/dist-manifest.json"

# Extract values from manifest
TAG=$(jq -r '.announcement_tag // .tag' "$MANIFEST")
TITLE=$(jq -r '.announcement_title' "$MANIFEST")
BODY=$(jq -r '.announcement_github_body' "$MANIFEST")
PRERELEASE=$(jq -r '.announcement_is_prerelease' "$MANIFEST")

# Write body to temp file
echo "$BODY" > /tmp/notes.txt

# Merge the publication artifacts and signed archives (like CI does).
mkdir -p artifacts
cp -r release-github-*/* build-github-archives-*/* "$SIGNED_ARCHIVES"/* artifacts/

# Remove the granular manifests (like CI does)
rm -f artifacts/*-dist-manifest.json

# Create release
release_args=(
    "$TAG"
    --target "$COMMIT"
    --title "$TITLE"
    --notes-file /tmp/notes.txt
)

if [ "$PRERELEASE" = "true" ]; then
    release_args+=(--prerelease)
fi

gh release create "${release_args[@]}" artifacts/*
