# Versioning

uv uses a custom versioning scheme in which the minor version number is bumped for breaking changes,
and the patch version number is bumped for bug fixes, enhancements, and other non-breaking changes.

uv is widely used in production. However, we value the ability to iterate on new features quickly
and gather changes that _could_ be breaking into clearly marked releases.

Once uv v1.0.0 is released, the versioning scheme will adhere to
[Semantic Versioning](https://semver.org/). There is not a particular goal that must be achieved for
uv to reach this milestone.

uv's changelog can be [viewed on GitHub](https://github.com/astral-sh/uv/blob/main/CHANGELOG.md).

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
