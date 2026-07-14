//! Integration tests for `uv lock`.

#[cfg(all(
    feature = "test-python",
    feature = "test-pypi",
    feature = "test-universal"
))]
use uv_test::pypi_proxy;

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
mod lock;

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
mod lock_build;

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
mod lock_build_frozen;

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
mod lock_runtime_consumers;
