#!/usr/bin/env sh
## Cargo wrapper that signs binaries after building via `cargo-code-sign`.
##
## Uses `CARGO_CODE_SIGN_CARGO` to determine the inner cargo command.
## If unset, falls back to plain `cargo`.
##
## Usage:
##
##   CARGO_CODE_SIGN_CARGO="$PWD/scripts/cargo-auditable.sh" \
##     scripts/cargo-code-sign.sh build --release

set -eu

exec cargo-code-sign code-sign "$@"
