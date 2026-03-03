#!/usr/bin/env sh
## Cargo wrapper that runs `cargo auditable` to embed SBOM metadata.
##
## Used as the inner build command for `cargo-code-sign`.
##
## Usage:
##
##   CARGO_CODE_SIGN_CARGO="$PWD/scripts/cargo-auditable.sh" cargo-code-sign code-sign ...

set -eu

exec cargo auditable "$@"
