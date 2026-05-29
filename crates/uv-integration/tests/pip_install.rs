//! Integration tests for `uv pip install`.

#[expect(
    dead_code,
    reason = "The pip install tests only use part of the shared proxy helper"
)]
#[path = "it/pypi_proxy.rs"]
mod pypi_proxy;

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
#[path = "it/pip_install.rs"]
mod pip_install;
