use std::path::PathBuf;

use anyhow::Result;
use miette::{Diagnostic, IntoDiagnostic};
use thiserror::Error;
use tracing::info;

use puffin_normalize::PackageName;
use puffin_workspace::WorkspaceError;

use crate::commands::ExitStatus;
use crate::printer::Printer;

/// Remove a dependency from the workspace.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn remove(name: &PackageName, _printer: Printer) -> Result<ExitStatus> {
    match remove_impl(name) {
        Ok(status) => Ok(status),
        Err(err) => {
            #[allow(clippy::print_stderr)]
            {
                eprint!("{err:?}");
            }
            Ok(ExitStatus::Failure)
        }
    }
}

#[derive(Error, Debug, Diagnostic)]
enum RemoveError {
    #[error(
        "Could not find a `pyproject.toml` file in the current directory or any of its parents"
    )]
    #[diagnostic(code(puffin::remove::workspace_not_found))]
    WorkspaceNotFound,

    #[error("Failed to parse `pyproject.toml` at: `{0}`")]
    #[diagnostic(code(puffin::remove::parse))]
    ParseError(PathBuf, #[source] WorkspaceError),

    #[error("Failed to write `pyproject.toml` to: `{0}`")]
    #[diagnostic(code(puffin::remove::write))]
    WriteError(PathBuf, #[source] WorkspaceError),

    #[error("Failed to remove `{0}` from `pyproject.toml`")]
    #[diagnostic(code(puffin::remove::parse))]
    RemovalError(String, #[source] WorkspaceError),
}

fn remove_impl(name: &PackageName) -> miette::Result<ExitStatus> {
    // Locate the workspace.
    let cwd = std::env::current_dir().into_diagnostic()?;
    let Some(workspace_root) = puffin_workspace::find_pyproject_toml(cwd) else {
        return Err(RemoveError::WorkspaceNotFound.into());
    };

    info!("Found workspace at: {}", workspace_root.display());

    // Parse the manifest.
    let mut manifest = puffin_workspace::Workspace::try_from(workspace_root.as_path())
        .map_err(|err| RemoveError::ParseError(workspace_root.clone(), err))?;

    // Remove the dependency.
    manifest
        .remove_dependency(name)
        .map_err(|err| RemoveError::RemovalError(name.to_string(), err))?;

    // Write the manifest back to disk.
    manifest
        .save(&workspace_root)
        .map_err(|err| RemoveError::WriteError(workspace_root.clone(), err))?;

    Ok(ExitStatus::Success)
}
