//! Integration tests for `uv lock`.

use uv_test::pypi_proxy;

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
mod lock;
