//! this is the single integration test, as documented by matklad
//! in <https://matklad.github.io/2021/02/27/delete-cargo-integration-tests.html>

pub(crate) mod common;

mod branching_urls;

#[cfg(all(feature = "python", feature = "pypi"))]
mod build;

#[cfg(feature = "python")]
mod build_backend;

#[cfg(all(feature = "python", feature = "pypi"))]
mod cache_clean;

#[cfg(all(feature = "python", feature = "pypi"))]
mod cache_prune;

#[cfg(all(feature = "python", feature = "pypi", feature = "test-ecosystem"))]
mod ecosystem;

#[cfg(all(feature = "python", feature = "pypi"))]
mod edit;

#[cfg(all(feature = "python", feature = "pypi"))]
mod export;

mod help;

#[cfg(all(feature = "python", feature = "pypi", feature = "git"))]
mod init;

#[cfg(all(feature = "python", feature = "pypi"))]
mod lock;

#[cfg(all(feature = "python", feature = "pypi"))]
mod lock_conflict;

mod lock_scenarios;

mod pip_check;

#[cfg(all(feature = "python", feature = "pypi"))]
mod pip_compile;

mod pip_compile_scenarios;

#[cfg(all(feature = "python", feature = "pypi"))]
mod pip_freeze;

#[cfg(all(feature = "python", feature = "pypi"))]
mod pip_install;

mod pip_install_scenarios;

mod pip_list;

mod pip_show;

#[cfg(all(feature = "python", feature = "pypi"))]
mod pip_sync;

mod pip_tree;
mod pip_uninstall;

#[cfg(feature = "pypi")]
mod publish;

mod python_dir;

#[cfg(feature = "python")]
mod python_find;

#[cfg(feature = "python-managed")]
mod python_install;

#[cfg(feature = "python")]
mod python_pin;

#[cfg(all(feature = "python", feature = "pypi"))]
mod run;

#[cfg(feature = "self-update")]
mod self_update;

#[cfg(all(feature = "python", feature = "pypi"))]
mod show_settings;

#[cfg(all(feature = "python", feature = "pypi"))]
mod sync;

#[cfg(all(feature = "python", feature = "pypi"))]
mod tool_dir;

#[cfg(all(feature = "python", feature = "pypi"))]
mod tool_install;

#[cfg(all(feature = "python", feature = "pypi"))]
mod tool_list;

#[cfg(all(feature = "python", feature = "pypi"))]
mod tool_run;

#[cfg(all(feature = "python", feature = "pypi"))]
mod tool_uninstall;

#[cfg(all(feature = "python", feature = "pypi"))]
mod tool_upgrade;

#[cfg(all(feature = "python", feature = "pypi"))]
mod tree;

#[cfg(feature = "python")]
mod venv;

#[cfg(all(feature = "python", feature = "pypi"))]
mod workflow;

mod workspace;
