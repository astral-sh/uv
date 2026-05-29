//! Miscellaneous integration tests for uv.

mod auth;

mod branching_urls;

#[cfg(all(
    feature = "test-python",
    feature = "test-pypi",
    feature = "test-ecosystem"
))]
mod ecosystem;

mod help;

mod network;

#[cfg(feature = "test-pypi")]
mod publish;

#[cfg(feature = "self-update")]
mod self_update;

mod version;

#[expect(
    dead_code,
    reason = "The miscellaneous tests only use part of the shared proxy helper"
)]
mod pypi_proxy;
