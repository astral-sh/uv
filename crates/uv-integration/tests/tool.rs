//! Integration tests for `uv tool`.

#[expect(
    dead_code,
    reason = "The tool tests only use part of the shared proxy helper"
)]
#[path = "it/pypi_proxy.rs"]
mod pypi_proxy;

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
#[path = "it/tool_dir.rs"]
mod tool_dir;

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
#[path = "it/tool_install.rs"]
mod tool_install;

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
#[path = "it/tool_list.rs"]
mod tool_list;

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
#[path = "it/tool_run.rs"]
mod tool_run;

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
#[path = "it/tool_uninstall.rs"]
mod tool_uninstall;

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
#[path = "it/tool_upgrade.rs"]
mod tool_upgrade;
