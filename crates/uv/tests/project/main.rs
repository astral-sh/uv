//! Integration tests for uv project commands.

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
use uv_test::pypi_proxy;

#[cfg(all(feature = "test-python", feature = "test-r2"))]
mod check;

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
mod edit;

#[cfg(all(
    feature = "test-python",
    feature = "test-pypi",
    feature = "test-universal"
))]
mod export;

#[cfg(all(feature = "test-python", feature = "test-r2"))]
mod format;

#[cfg(all(feature = "test-python", feature = "test-pypi", feature = "test-git"))]
mod init;

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
mod run;

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
mod tree;

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
mod workflow;
