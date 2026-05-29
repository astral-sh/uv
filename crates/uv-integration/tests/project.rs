//! Integration tests for uv project commands.

#[expect(
    dead_code,
    reason = "The project tests only use part of the shared proxy helper"
)]
#[path = "it/pypi_proxy.rs"]
mod pypi_proxy;

#[cfg(all(feature = "test-python", feature = "test-r2"))]
#[path = "it/check.rs"]
mod check;

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
#[path = "it/edit.rs"]
mod edit;

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
#[path = "it/export.rs"]
mod export;

#[cfg(all(feature = "test-python", feature = "test-r2"))]
#[path = "it/format.rs"]
mod format;

#[cfg(all(feature = "test-python", feature = "test-pypi", feature = "test-git"))]
#[path = "it/init.rs"]
mod init;

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
#[path = "it/run.rs"]
mod run;

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
#[path = "it/tree.rs"]
mod tree;

#[cfg(all(feature = "test-python", feature = "test-pypi"))]
#[path = "it/workflow.rs"]
mod workflow;
