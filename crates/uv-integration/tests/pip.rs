//! Integration tests for uv pip commands.

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
#[path = "it/pip_check.rs"]
mod pip_check;

#[path = "it/pip_compile_scenarios.rs"]
mod pip_compile_scenarios;

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
#[path = "it/pip_exclude_newer_relative.rs"]
mod pip_exclude_newer_relative;

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
#[path = "it/pip_freeze.rs"]
mod pip_freeze;

#[path = "it/pip_install_scenarios.rs"]
mod pip_install_scenarios;

#[path = "it/pip_list.rs"]
mod pip_list;

#[path = "it/pip_show.rs"]
mod pip_show;

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
#[path = "it/pip_sync.rs"]
mod pip_sync;

#[path = "it/pip_debug.rs"]
mod pip_debug;

#[path = "it/pip_tree.rs"]
mod pip_tree;

#[path = "it/pip_uninstall.rs"]
mod pip_uninstall;
