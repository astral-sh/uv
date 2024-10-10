//! this is the single integration test, as documented by matklad
//! in <https://matklad.github.io/2021/02/27/delete-cargo-integration-tests.html>

pub(crate) mod common;

mod build;
mod build_backend;
mod cache_clean;
mod cache_prune;
mod ecosystem;
mod edit;
mod export;
mod help;
mod init;
mod lock;
mod lock_scenarios;
mod pip_check;
mod pip_compile;
mod pip_compile_scenarios;
mod pip_freeze;
mod pip_install;
mod pip_install_scenarios;
mod pip_list;
mod pip_show;
mod pip_sync;
mod pip_tree;
mod pip_uninstall;
mod publish;
mod python_dir;
mod python_find;
mod python_pin;
mod run;
mod self_update;
mod show_settings;
mod sync;
mod tool_dir;
mod tool_install;
mod tool_list;
mod tool_run;
mod tool_uninstall;
mod tool_upgrade;
mod tree;
mod venv;
mod workflow;
mod workspace;
