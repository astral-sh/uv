//! Integration tests for `uv pip compile`.

#[expect(
    dead_code,
    reason = "The pip compile tests only use part of the shared proxy helper"
)]
#[path = "it/pypi_proxy.rs"]
mod pypi_proxy;

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
#[path = "it/pip_compile.rs"]
mod pip_compile;
