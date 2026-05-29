//! Integration tests for `uv lock`.

#[expect(
    dead_code,
    reason = "The lock tests only use part of the shared proxy helper"
)]
#[path = "it/pypi_proxy.rs"]
mod pypi_proxy;

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
#[path = "it/lock.rs"]
mod lock;
