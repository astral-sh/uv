# Versioning

uv is widely used in production and is stable software.

uv uses a custom versioning scheme in which the minor version number is bumped for breaking changes,
and the patch version number is bumped for bug fixes, enhancements, and other non-breaking changes.

The care we take in backwards-incompatible changes is proportional to the expected real-world
impact, not a function of arbitrary version numbering policies. We value the ability to iterate on
new features quickly and gather changes that _could_ be breaking into clearly marked releases.

uv's changelog can be [viewed on GitHub](https://github.com/astral-sh/uv/blob/main/CHANGELOG.md).

## Crate versioning

uv's crates are published to [crates.io](https://crates.io). The following crates follow the normal
uv versioning policy:

- `uv`
- `uv-build`
- `uv-version`

The `uv` and `uv-build` crates are versioned by the binary command-line interface. The Rust
interface of these crates does not follow semantic versioning.

The remainder of uv's crates provide **no stability guarantees**. The Rust interface is considered
internal and unstable. Consequently, they are versioned as `0.0.x`. The patch version is incremented
on every uv release, regardless of changes to the crate.

## Cache versioning

Cache versions are considered internal to uv, and so may be changed in a minor or patch release. See
[Cache versioning](../../concepts/cache.md#cache-versioning) for more.

## Lockfile versioning

The `uv.lock` schema version is considered part of the public API, and so will only be incremented
in a minor release as a breaking change. See
[Lockfile versioning](../../concepts/resolution.md#lockfile-versioning) for more.

## Minimum supported Rust version

The minimum supported Rust version required to compile uv is listed in the `rust-version` key of the
`[workspace.package]` section in `Cargo.toml`. It may change in any release (minor or patch). It
will never be newer than N-2 Rust versions, where N is the latest stable version. For example, if
the latest stable Rust version is 1.85, uv's minimum supported Rust version will be at most 1.83.

This is only relevant to users who build uv from source. Installing uv from the Python package index
usually installs a pre-built binary and does not require Rust compilation.
