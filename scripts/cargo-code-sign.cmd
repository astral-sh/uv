@echo off
REM Cargo wrapper that signs binaries after building via `cargo-code-sign`.
REM See cargo-code-sign.sh for the full explanation.

cargo-code-sign.exe code-sign %*
