//! Integration tests for `uv pip compile`.

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
use uv_test::pypi_proxy;

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
#[path = "it/pip_compile.rs"]
mod pip_compile;
