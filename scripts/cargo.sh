#!/usr/bin/env sh
## Wrapper script that invokes `cargo auditable` instead of plain `cargo`.
##
## Use `scripts/install-cargo-extensions.sh` to install the dependencies.
##
## Usage:
##
##   CARGO="$PWD/scripts/cargo.sh" cargo build --release

set -eu

if [ -n "${REAL_CARGO:-}" ]; then
    exec "$REAL_CARGO" auditable "$@"
else
    exec cargo auditable "$@"
fi
