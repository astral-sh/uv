#!/usr/bin/env bash
#
# Manually perform a GitHub release for a broken automated release that otherwise went out.
#
# PLEASE USE WITH CAUTION.
#
# It can take a while to download all the artifacts.
#
# Requires `gh` and `uv`.

set -euo pipefail
SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)

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

# Download exactly the publication artifacts from the same run attempt.
RUN_ATTEMPT=$(gh run view "$RUN_ID" --repo "$REPO" --json attempt --jq .attempt)
for artifact in archives global manifest; do
    gh run download "$RUN_ID" --repo "$REPO" \
        --name "release-github-$artifact-$RUN_ID-$RUN_ATTEMPT" --dir artifacts
done

MANIFEST="artifacts/dist-manifest.json"
uv run --locked "$SCRIPT_DIR/list-release-artifacts.py" github "$MANIFEST" artifacts --include-manifest > release-assets.txt
assets=()
while IFS= read -r asset; do
    assets+=("$asset")
done < release-assets.txt

# Extract values from manifest
TAG=$(jq -r '.announcement_tag // .tag' "$MANIFEST")
TITLE=$(jq -r '.announcement_title' "$MANIFEST")
BODY=$(jq -r '.announcement_github_body' "$MANIFEST")
PRERELEASE=$(jq -r '.announcement_is_prerelease' "$MANIFEST")

# Write body to temp file
echo "$BODY" > notes.txt

# Create release
release_args=(
    "$TAG"
    --target "$COMMIT"
    --title "$TITLE"
    --notes-file notes.txt
)

if [ "$PRERELEASE" = "true" ]; then
    release_args+=(--prerelease)
fi

gh release create "${release_args[@]}" "${assets[@]}"
