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
mod sys_abi_features;
