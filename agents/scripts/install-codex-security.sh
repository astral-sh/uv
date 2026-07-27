#!/usr/bin/env bash
set -euo pipefail

mkdir -p "$RUNNER_TEMP/codex-security"
curl --fail --location --silent --show-error \
  --output "$RUNNER_TEMP/codex.tar.gz" \
  https://github.com/openai/codex/releases/download/rust-v0.144.4/codex-x86_64-unknown-linux-musl.tar.gz
printf '%s  %s\n' \
  37c985be9d89e8c4f43b3aa0594c1213eac212d30ae2b95221f08fec807515d1 \
  "$RUNNER_TEMP/codex.tar.gz" | sha256sum --check
tar --extract --gzip --file "$RUNNER_TEMP/codex.tar.gz" --directory "$RUNNER_TEMP"
codex="$RUNNER_TEMP/codex-x86_64-unknown-linux-musl"

marketplace=.codex-security-plugin/.agents/plugins/marketplace.json
jq '.name = "openai-curated-ci" | .plugins |= map(select(.name == "codex-security"))' \
  "$marketplace" > "$RUNNER_TEMP/marketplace.json"
mv "$RUNNER_TEMP/marketplace.json" "$marketplace"

CODEX_HOME="$GITHUB_WORKSPACE/agents/codex" "$codex" plugin marketplace add \
  "$GITHUB_WORKSPACE/.codex-security-plugin"
CODEX_HOME="$GITHUB_WORKSPACE/agents/codex" "$codex" plugin add \
  codex-security@openai-curated-ci
