#!/usr/bin/env sh
## Install cargo extensions for release builds.
##
## Installs cargo-auditable for SBOM embedding and cargo-code-sign for binary signing.
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

CARGO_CODE_SIGN_INSTALL="cargo install cargo-code-sign \
    --locked \
    --git https://github.com/zanieb/cargo-code-sign \
    --rev 3448dce9525127604dc65db1dc2a5f4b67f214b6"

# In Linux containers running on x86_64, build a static musl binary so the installed tool works in
# musl-based environments (Alpine, etc.).
#
# On i686 containers the 32-bit linker can't produce 64-bit musl binaries, so we fall back to a
# default build.
if [ "$(uname -m 2>/dev/null)" = "x86_64" ] && [ "$(uname -s 2>/dev/null)" = "Linux" ]; then
    MUSL_TARGET="x86_64-unknown-linux-musl"
    rustup target add "$MUSL_TARGET"
    CC=gcc $CARGO_AUDITABLE_INSTALL --target "$MUSL_TARGET"
    CC=gcc $CARGO_CODE_SIGN_INSTALL --target "$MUSL_TARGET"
else
    $CARGO_AUDITABLE_INSTALL
    $CARGO_CODE_SIGN_INSTALL
fi
