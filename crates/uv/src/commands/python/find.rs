use anyhow::Result;
use std::fmt::Write;
use std::path::Path;

use uv_cache::Cache;
use uv_fs::Simplified;
use uv_python::{
    EnvironmentPreference, PythonDownloads, PythonInstallation, PythonPreference, PythonRequest,
};
use uv_scripts::Pep723ItemRef;
use uv_settings::PythonInstallMirrors;
use uv_warnings::{warn_user, warn_user_once};
use uv_workspace::{DiscoveryOptions, VirtualProject, WorkspaceCache, WorkspaceError};

use crate::commands::{
    ExitStatus,
    project::{ScriptInterpreter, WorkspacePython, validate_project_requires_python},
};
use crate::printer::Printer;
use crate::settings::NetworkSettings;

/// Find a Python interpreter.
#[allow(clippy::fn_params_excessive_bools)]
pub(crate) async fn find(
    project_dir: &Path,
    request: Option<String>,
    show_version: bool,
    no_project: bool,
    no_config: bool,
    system: bool,
    python_preference: PythonPreference,
    cache: &Cache,
    printer: Printer,
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
    }

    if show_version {
        writeln!(
            printer.stdout(),
            "{}",
            python.interpreter().python_version()
        )?;
    } else {
        writeln!(
            printer.stdout(),
            "{}",
            std::path::absolute(python.interpreter().sys_executable())?.simplified_display()
        )?;
    }

    Ok(ExitStatus::Success)
}

pub(crate) async fn find_script(
    script: Pep723ItemRef<'_>,
    show_version: bool,
    network_settings: &NetworkSettings,
    python_preference: PythonPreference,
    python_downloads: PythonDownloads,
    no_config: bool,
    cache: &Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    let interpreter = match ScriptInterpreter::discover(
        script,
        None,
        network_settings,
        python_preference,
        python_downloads,
        &PythonInstallMirrors::default(),
        false,
        no_config,
        Some(false),
        cache,
        printer,
    )
    .await
    {
        Err(error) => {
            writeln!(printer.stderr(), "{error}")?;
            return Ok(ExitStatus::Failure);
        }
        Ok(ScriptInterpreter::Interpreter(interpreter)) => interpreter,
        Ok(ScriptInterpreter::Environment(environment)) => environment.into_interpreter(),
    };

    if show_version {
        writeln!(printer.stdout(), "{}", interpreter.python_version())?;
    } else {
        writeln!(
            printer.stdout(),
            "{}",
            std::path::absolute(interpreter.sys_executable())?.simplified_display()
        )?;
    }

    Ok(ExitStatus::Success)
}
