#!/usr/bin/env bash
set -euo pipefail

codex_home="${1:?Codex home directory is required}"

mkdir -p "$RUNNER_TEMP/codex-security"
curl --fail --location --silent --show-error \
  --output "$RUNNER_TEMP/codex.tar.gz" \
  https://github.com/openai/codex/releases/download/rust-v0.153.4/codex-x86_64-unknown-linux-musl.tar.gz
printf '%s  %s\n' \
  f479424eca092484dc40d87ae28c44f4cc40234a60045d6131e493800d814a30 \
  "$RUNNER_TEMP/codex.tar.gz" | sha256sum --check
tar --extract --gzip --file "$RUNNER_TEMP/codex.tar.gz" --directory "$RUNNER_TEMP"
codex="$RUNNER_TEMP/codex-x86_64-unknown-linux-musl"

marketplace=.codex-security-plugin/.agents/plugins/marketplace.json
jq '.name = "openai-curated-ci" | .plugins |= map(select(.name == "codex-security"))' \
  "$marketplace" > "$RUNNER_TEMP/marketplace.json"
mv "$RUNNER_TEMP/marketplace.json" "$marketplace"

CODEX_HOME="$codex_home" "$codex" plugin marketplace add \
  "$GITHUB_WORKSPACE/.codex-security-plugin"
CODEX_HOME="$codex_home" "$codex" plugin add \
  codex-security@openai-curated-ci
