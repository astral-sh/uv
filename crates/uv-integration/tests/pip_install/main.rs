//! Integration tests for `uv pip install`.

use uv_test::pypi_proxy;

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
mod pip_install;
