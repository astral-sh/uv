#!/usr/bin/env bash
set -euo pipefail

: "${HEAD_SHA:?Pull request head SHA is required}"
: "${THREAT_MODELS:?Threat model directory is required}"

# Save configuration and helpers before switching to the PR head.
review_source="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
review_config="$RUNNER_TEMP/pull-request-review"
printf 'REVIEW_CONFIG=%s\n' "$review_config" >> "$GITHUB_ENV"
mkdir -p "$review_config/codex" "$review_config/references"
cp "$review_source/config.toml" "$review_config/codex/config.toml"
cp "$review_source/prompt.md" "$review_source/schema.json" "$review_config/"
cp -R "$review_source/scripts" "$review_config/"
cp -R "$THREAT_MODELS/." "$review_config/references/"
git rev-parse HEAD > "$review_config/configuration-revision.txt"
printf 'config-directory=%s\n' "$review_config" >> "$GITHUB_OUTPUT"
git checkout --detach "$HEAD_SHA"
