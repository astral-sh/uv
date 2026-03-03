//! this is the single integration test, as documented by matklad
//! in <https://matklad.github.io/2021/02/27/delete-cargo-integration-tests.html>

mod auth;

mod branching_urls;

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
mod build;

#[cfg(feature = "test-python")]
mod build_backend;

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
mod cache;

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
mod cache_clean;

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
mod cache_prune;

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
mod cache_size;

#[cfg(all(
    feature = "test-python",
    feature = "test-pypi",
    feature = "test-ecosystem"
))]
mod ecosystem;

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
mod edit;

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
mod export;

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
mod format;

mod help;

#[cfg(all(feature = "test-python", feature = "test-pypi", feature = "test-git"))]
mod init;

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
mod lock;

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
mod lock_conflict;

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
mod lock_exclude_newer_relative;

mod lock_scenarios;

mod network;

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
mod pip_check;

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
mod pip_compile;

mod pip_compile_scenarios;

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
mod pip_freeze;

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
mod pip_install;

mod pip_install_scenarios;

mod pip_list;

mod pip_show;

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
mod pip_sync;

mod pip_debug;
mod pip_tree;
mod pip_uninstall;

#[cfg(feature = "test-pypi")]
mod publish;

mod python_dir;

#[cfg(feature = "test-python")]
mod python_find;

#[cfg(feature = "test-python")]
mod python_list;

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
mod python_module;

#[cfg(feature = "test-python-managed")]
mod python_install;

#[cfg(feature = "test-python")]
mod python_pin;

#[cfg(feature = "test-python-managed")]
mod python_upgrade;

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
mod run;

#[cfg(feature = "self-update")]
mod self_update;

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
mod show_settings;

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
mod sync;

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

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
mod tree;

#[cfg(feature = "test-python")]
mod venv;

mod version;

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
mod workflow;

mod extract;
mod pypi_proxy;
mod workspace;
mod workspace_dir;
mod workspace_list;
mod workspace_metadata;
