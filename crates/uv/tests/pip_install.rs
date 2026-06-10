//! Integration tests for `uv pip install`.

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
use uv_test::pypi_proxy;

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
#[path = "it/pip_install.rs"]
mod pip_install;
