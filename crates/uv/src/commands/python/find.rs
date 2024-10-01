use anstream::println;
use anyhow::Result;
use std::path::Path;

use uv_cache::Cache;
use uv_fs::Simplified;
use uv_python::{EnvironmentPreference, PythonInstallation, PythonPreference};

use crate::commands::{project::python_request_from_args, ExitStatus};

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

    let request = python_request_from_args(
        request.as_deref(),
        no_project,
        no_config,
        Some(project_dir),
        None,
    )
    .await?;

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
