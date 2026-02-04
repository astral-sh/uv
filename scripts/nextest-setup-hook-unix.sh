#!/usr/bin/env bash
#
# Nextest setup hook.
#
# Runs before tests execute. Add any pre-test setup steps here.
#
# Usage:
#
#   This script is run automatically by nextest before tests. It is configured in
#   `.config/nextest.toml` as a setup script.

set -euo pipefail

# macOS code signing
#
# On macOS, binaries must be signed to access the system keychain without prompts after each re-compile.
# This is required when running tests with the `native-auth` feature.
# Set UV_TEST_CODESIGN_IDENTITY to enable signing. See `scripts/codesign-macos.sh`.
if [[ "$(uname)" == "Darwin" && -n "${UV_TEST_CODESIGN_IDENTITY:-}" ]]; then
  SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

  targets=()

  for bin in target/debug/uv target/debug/uvx; do
    [[ -f "$bin" ]] && targets+=("$bin")
  done

  while IFS= read -r -d '' bin; do
    targets+=("$bin")
  done < <(find target/debug/deps -type f -perm +111 ! -name "*.d" ! -name "*.dylib" -print0 2>/dev/null || true)

  if [[ ${#targets[@]} -gt 0 ]]; then
    "$SCRIPT_DIR/codesign-macos.sh" "$UV_TEST_CODESIGN_IDENTITY" "${targets[@]}"
  fi
fi
