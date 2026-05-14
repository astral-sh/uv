# uv-netrc

This crate vendors the [`rust-netrc`](https://crates.io/crates/rust-netrc) parser for use by uv.

The source was vendored from
[`gribouille/netrc`](https://github.com/gribouille/netrc/tree/e5f96cc9cc78931b949b48c0758f15da331e9761),
as published in `rust-netrc` 0.1.2 with checksum
`7e98097f62769f92dbf95fb51f71c0a68ec18a4ee2e70e0d3e4f47ac005d63e9`.

The package is named `uv-netrc`, but the library target remains `netrc` to match the upstream API
surface.

## License

The vendored source is licensed under MIT. See [LICENSE](./LICENSE).

## Patches

The following changes have been applied:

- Renamed the package to `uv-netrc`.
- Adopted uv workspace metadata and lints.
- Uses uv's workspace `thiserror` dependency.
- Uses uv's workspace `fs-err` dependency in tests.
- Uses uv's workspace `temp-env` test helper for environment-variable tests.
- Restricted internal lexer visibility to satisfy uv's workspace lints.
- Applied lint-only style changes for uv's workspace clippy configuration.
