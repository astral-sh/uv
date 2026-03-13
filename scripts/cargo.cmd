@echo off
REM Wrapper script that invokes `cargo auditable` instead of plain `cargo`.
REM
REM Use `scripts/install-cargo-extensions.sh` to install the dependencies.
REM
REM Usage:
REM
REM   set CARGO=%CD%\scripts\cargo.cmd
REM   cargo build --release

if defined REAL_CARGO (
    "%REAL_CARGO%" auditable %*
) else (
    cargo.exe auditable %*
)
