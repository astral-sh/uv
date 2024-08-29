use anstream::println;
use anyhow::Result;

use uv_cache::Cache;
use uv_fs::{Simplified, CWD};
use uv_python::{
    EnvironmentPreference, PythonInstallation, PythonPreference, PythonRequest, PythonVersionFile,
    VersionRequest,
};
use uv_resolver::RequiresPython;
use uv_warnings::warn_user_once;
use uv_workspace::{DiscoveryOptions, VirtualProject, WorkspaceError};

use crate::commands::{project::find_requires_python, ExitStatus};

/// Find a Python interpreter.
pub(crate) async fn find(
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

    // (1) Explicit request from user
    let mut request = request.map(|request| PythonRequest::parse(&request));

    // (2) Request from `.python-version`
    if request.is_none() {
        request = PythonVersionFile::discover(&*CWD, no_config, false)
            .await?
            .and_then(PythonVersionFile::into_version);
    }

    // (3) `Requires-Python` in `pyproject.toml`
    if request.is_none() && !no_project {
        let project = match VirtualProject::discover(&CWD, &DiscoveryOptions::default()).await {
            Ok(project) => Some(project),
            Err(WorkspaceError::MissingProject(_)) => None,
            Err(WorkspaceError::MissingPyprojectToml) => None,
            Err(WorkspaceError::NonWorkspace(_)) => None,
            Err(err) => {
                warn_user_once!("{err}");
                None
            }
        };

        if let Some(project) = project {
            request = find_requires_python(project.workspace())?
                .as_ref()
                .map(RequiresPython::specifiers)
                .map(|specifiers| {
                    PythonRequest::Version(VersionRequest::Range(specifiers.clone()))
                });
        }
    }

    let python = PythonInstallation::find(
        &request.unwrap_or_default(),
        environment_preference,
        python_preference,
        cache,
    )?;

    println!(
        "{}",
        std::path::absolute(python.interpreter().sys_executable())?.simplified_display()
    );

    Ok(ExitStatus::Success)
}
