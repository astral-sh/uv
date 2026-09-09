#!/usr/bin/env bash
#
# Manually perform a GitHub release for a broken automated release that otherwise went out.
#
# PLEASE USE WITH CAUTION.
#
# It can take a while to download all the artifacts.
#
# Requires `gh` and `jq`.

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

# Find the publication artifacts across all attempts of this run.
gh api --paginate "repos/$REPO/actions/runs/$RUN_ID/artifacts?per_page=100" |
    jq -s '[.[].artifacts[]]' > workflow-artifacts.json
RUN_ATTEMPT=$(gh run view "$RUN_ID" --repo "$REPO" --json attempt --jq .attempt)

# Return the latest artifact in a family, up to the given run attempt.
artifact_name() {
    jq -er --arg prefix "release-github-$1-$RUN_ID-" --argjson attempt "$2" '
        [.[]
         | select(.name | startswith($prefix))
         | . + {attempt: (.name | ltrimstr($prefix) | tonumber)}
         | select(.attempt <= $attempt)]
        | max_by(.attempt)
        | if . == null then error("Missing artifact: \($prefix)*")
          elif .expired then error("Artifact has expired: \(.name)")
          else .name end
    ' workflow-artifacts.json
}

# Successful upstream jobs may come from earlier attempts. Anchor their selection
# to the last hosted manifest, excluding outputs from any later failed rerun.
manifest_artifact=$(artifact_name manifest "$RUN_ATTEMPT")
manifest_attempt=${manifest_artifact##*-}
gh run download "$RUN_ID" --repo "$REPO" --name "$manifest_artifact" --dir artifacts
for artifact in archives global; do
    name=$(artifact_name "$artifact" "$manifest_attempt")
    gh run download "$RUN_ID" --repo "$REPO" \
        --name "$name" --dir artifacts
done

MANIFEST="artifacts/dist-manifest.json"
jq -r '(["dist-manifest.json"] + [.releases[].artifacts[]])[] | "artifacts/\(.)"' "$MANIFEST" > release-assets.txt
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
