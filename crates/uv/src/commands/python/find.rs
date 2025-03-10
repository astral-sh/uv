use anstream::println;
use anyhow::Result;
use std::path::Path;

use uv_cache::Cache;
use uv_fs::Simplified;
use uv_python::{EnvironmentPreference, PythonInstallation, PythonPreference, PythonRequest};
use uv_warnings::{warn_user, warn_user_once};
use uv_workspace::{DiscoveryOptions, VirtualProject, WorkspaceCache, WorkspaceError};

use crate::commands::{
    project::{validate_project_requires_python, WorkspacePython},
    ExitStatus,
};

/// Find a Python interpreter.
pub(crate) async fn find(
    project_dir: &Path,
    request: Option<String>,
    no_project: bool,
    no_config: bool,
    system: bool,
    python_preference: PythonPreference,
    cache: &Cache,
) -> Result<ExitStatus> {
    let environment_preference = if system {
        EnvironmentPreference::OnlySystem
    } else {
        EnvironmentPreference::Any
    };

    let workspace_cache = WorkspaceCache::default();
    let project = if no_project {
        None
    } else {
        match VirtualProject::discover(project_dir, &DiscoveryOptions::default(), &workspace_cache)
            .await
        {
            Ok(project) => Some(project),
            Err(WorkspaceError::MissingProject(_)) => None,
            Err(WorkspaceError::MissingPyprojectToml) => None,
            Err(WorkspaceError::NonWorkspace(_)) => None,
            Err(err) => {
                warn_user_once!("{err}");
                None
            }
        }
    };

    let WorkspacePython {
        source,
        python_request,
        requires_python,
    } = WorkspacePython::from_request(
        request.map(|request| PythonRequest::parse(&request)),
        project.as_ref().map(VirtualProject::workspace),
        project_dir,
        no_config,
    )
    .await?;

    let python = PythonInstallation::find(
        &python_request.unwrap_or_default(),
        environment_preference,
        python_preference,
        cache,
    )?;

    // Warn if the discovered Python version is incompatible with the current workspace
    if let Some(requires_python) = requires_python {
        match validate_project_requires_python(
            python.interpreter(),
            project.as_ref().map(VirtualProject::workspace),
            &requires_python,
            &source,
        ) {
            Ok(()) => {}
            Err(err) => {
                warn_user!("{err}");
            }
        }
    };

    println!(
        "{}",
        std::path::absolute(python.interpreter().sys_executable())?.simplified_display()
    );

    Ok(ExitStatus::Success)
}
