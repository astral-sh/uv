//! Integration tests for `uv tool`.

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
use uv_test::pypi_proxy;

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
mod tool_dir;

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
mod tool_install;

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
mod tool_list;

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
mod tool_run;

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
mod tool_uninstall;

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
mod tool_upgrade;
