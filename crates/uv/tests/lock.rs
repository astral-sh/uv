//! Integration tests for `uv lock`.

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
use uv_test::pypi_proxy;

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
#[path = "it/lock.rs"]
mod lock;
