#!/usr/bin/env sh
## Install cargo extensions for release builds.
##
## Installs cargo-auditable for SBOM embedding.
##
## Includes handling for cross-build containers in our release workflow.
##
## Usage:
##
##   $ scripts/install-cargo-extensions.sh
##
## Expected to be used with `scripts/cargo.sh`.

set -eu

CARGO_AUDITABLE_INSTALL="cargo install cargo-auditable \
    --locked \
    --version 0.7.4"

# In Linux containers running on x86_64, build a static musl binary so the installed tool works in
# musl-based environments (Alpine, etc.).
#
# On i686 containers the 32-bit linker can't produce 64-bit musl binaries, so we fall back to a
# default build.
if [ "$(uname -m 2>/dev/null)" = "x86_64" ] && [ "$(uname -s 2>/dev/null)" = "Linux" ]; then
    MUSL_TARGET="x86_64-unknown-linux-musl"
    rustup target add "$MUSL_TARGET"
    CC=gcc $CARGO_AUDITABLE_INSTALL --target "$MUSL_TARGET"
else
    $CARGO_AUDITABLE_INSTALL
fi
