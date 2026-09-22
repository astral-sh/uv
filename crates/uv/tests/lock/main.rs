//! Integration tests for `uv lock`.

#[cfg(all(
    feature = "test-python",
    feature = "test-pypi",
    feature = "test-universal"
))]
use uv_test::pypi_proxy;

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
mod lock;

#[cfg(all(feature = "test-python", feature = "test-universal"))]
mod minimum_libc;

#[cfg(all(feature = "test-python", feature = "test-universal"))]
mod resolution_inputs;
