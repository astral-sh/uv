use std::path::PathBuf;

use anyhow::Result;
use miette::{Diagnostic, IntoDiagnostic};
use thiserror::Error;
use tracing::info;

use puffin_workspace::WorkspaceError;

use crate::commands::ExitStatus;
use crate::printer::Printer;

/// Add a dependency to the workspace.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn add(name: &str, _printer: Printer) -> Result<ExitStatus> {
    match add_impl(name) {
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
enum AddError {
    #[error(
        "Could not find a `pyproject.toml` file in the current directory or any of its parents"
    )]
    #[diagnostic(code(puffin::add::workspace_not_found))]
    WorkspaceNotFound,

    #[error("Failed to parse requirement: `{0}`")]
    #[diagnostic(code(puffin::add::invalid_requirement))]
    InvalidRequirement(String, #[source] pep508_rs::Pep508Error),

    #[error("Failed to parse `pyproject.toml` at: `{0}`")]
    #[diagnostic(code(puffin::add::parse))]
    ParseError(PathBuf, #[source] WorkspaceError),

    #[error("Failed to write `pyproject.toml` to: `{0}`")]
    #[diagnostic(code(puffin::add::write))]
    WriteError(PathBuf, #[source] WorkspaceError),
}

fn add_impl(name: &str) -> miette::Result<ExitStatus> {
    let requirement = puffin_workspace::VerbatimRequirement::try_from(name)
        .map_err(|err| AddError::InvalidRequirement(name.to_string(), err))?;

    // Locate the workspace.
    let cwd = std::env::current_dir().into_diagnostic()?;
    let Some(workspace_root) = puffin_workspace::find_pyproject_toml(cwd) else {
        return Err(AddError::WorkspaceNotFound.into());
    };

    info!("Found workspace at: {}", workspace_root.display());

    // Parse the manifest.
    let mut manifest = puffin_workspace::Workspace::try_from(workspace_root.as_path())
        .map_err(|err| AddError::ParseError(workspace_root.clone(), err))?;

    // Add the dependency.
    manifest.add_dependency(&requirement);

    // Write the manifest back to disk.
    manifest
        .save(&workspace_root)
        .map_err(|err| AddError::WriteError(workspace_root.clone(), err))?;

    Ok(ExitStatus::Success)
}
