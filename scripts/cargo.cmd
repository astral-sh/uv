@echo off
REM Top-level cargo wrapper for release builds.
REM
REM Chains `cargo-code-sign` (post-build binary signing) with `cargo-auditable`
REM (SBOM embedding). See cargo.sh for the full explanation.

set CARGO_CODE_SIGN_CARGO=%~dp0cargo-auditable.cmd
%~dp0cargo-code-sign.cmd %*
