#!/usr/bin/env sh
## Top-level cargo wrapper for release builds.
##
## Chains `cargo-code-sign` (post-build binary signing) with `cargo-auditable`
## (SBOM embedding):
##
##   maturin -> cargo.sh -> cargo-code-sign -> cargo-auditable -> cargo
##
## Use `scripts/install-cargo-extensions.sh` to install the dependencies.
##
## Usage:
##
##   CARGO="$PWD/scripts/cargo.sh" maturin build --release

set -eu

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

# Tell cargo-code-sign to use cargo-auditable as the inner build command.
export CARGO_CODE_SIGN_CARGO="${SCRIPT_DIR}/cargo-auditable.sh"

exec "${SCRIPT_DIR}/cargo-code-sign.sh" "$@"
