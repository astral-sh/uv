# Rust support

The minimum supported Rust version required to compile uv is listed in the `rust-version` key of the
`[workspace.package]` section in `Cargo.toml`. It may change in any release (minor or patch). It
will never be newer than N-2 Rust versions, where N is the latest stable version. For example, if
the latest stable Rust version is 1.85, uv's minimum supported Rust version will be at most 1.83.

This is only relevant to users who build uv from source. Installing uv from the Python package index
usually installs a pre-built binary and does not require Rust compilation.
