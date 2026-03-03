@echo off
REM Cargo wrapper that runs `cargo auditable` to embed SBOM metadata.
REM See cargo-auditable.sh for the full explanation.

cargo.exe auditable %*
