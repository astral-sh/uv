use std::fmt::Write;
use std::path::PathBuf;

use anyhow::{bail, Result};

use tracing::debug;
use uv_cache::Cache;
use uv_configuration::PreviewMode;
use uv_distribution::VirtualProject;
use uv_fs::Simplified;
use uv_python::{
    requests_from_version_file, EnvironmentPreference, PythonInstallation, PythonPreference,
    PythonRequest, PYTHON_VERSION_FILENAME,
};
use uv_warnings::warn_user_once;

use crate::commands::{project::find_requires_python, ExitStatus};
use crate::printer::Printer;

/// Pin to a specific Python version.
pub(crate) async fn pin(
    request: Option<String>,
    resolved: bool,
    python_preference: PythonPreference,
    preview: PreviewMode,
    isolated: bool,
    cache: &Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    if preview.is_disabled() {
        warn_user_once!("`uv python pin` is experimental and may change without warning.");
    }

    let Some(request) = request else {
        // Display the current pinned Python version
        if let Some(pins) = requests_from_version_file().await? {
            for pin in pins {
                writeln!(printer.stdout(), "{}", pin.to_canonical_string())?;
            }
            return Ok(ExitStatus::Success);
        }
        bail!("No pinned Python version found.")
    };
    let request = PythonRequest::parse(&request);

    let python = match PythonInstallation::find(
        &request,
        EnvironmentPreference::OnlySystem,
        python_preference,
        cache,
    ) {
        Ok(python) => Some(python),
        // If no matching Python version is found, don't fail unless `resolved` was requested
        Err(uv_python::Error::MissingPython(err)) if !resolved => {
            warn_user_once!("{}", err);
            None
        }
        Err(err) => return Err(err.into()),
    };

    if !isolated {
        if let Ok(project) = VirtualProject::discover(&std::env::current_dir()?, None).await {
            let (workspace, project_type) = match project {
                VirtualProject::Project(project_workspace) => {
                    debug!(
                        "Discovered project `{}` at: {}",
                        project_workspace.project_name(),
                        project_workspace.workspace().install_path().display()
                    );
                    (project_workspace.workspace().clone(), "project")
                }
                VirtualProject::Virtual(workspace) => {
                    debug!(
                        "Discovered virtual workspace at: {}",
                        workspace.install_path().display()
                    );
                    (workspace, "virtual")
                }
            };
            let requires_python = find_requires_python(&workspace)?;
            let python_version = python
                .as_ref()
                .map(uv_python::PythonInstallation::python_version);
            if let (Some(requires_python), Some(python_version)) = (requires_python, python_version)
            {
                if !requires_python.contains(python_version) {
                    anyhow::bail!("The pinned Python version is incompatible with the {project_type}'s `Requires-Python` of {requires_python}.");
                }
            }
        }
    }

    let output = if resolved {
        // SAFETY: We exit early if Python is not found and resolved is `true`
        python
            .unwrap()
            .interpreter()
            .sys_executable()
            .user_display()
            .to_string()
    } else {
        request.to_canonical_string()
    };

    debug!("Using pin `{}`", output);
    let version_file = PathBuf::from(PYTHON_VERSION_FILENAME);
    let exists = version_file.exists();

    debug!("Writing pin to {}", version_file.user_display());
    fs_err::write(&version_file, format!("{output}\n"))?;
    if exists {
        writeln!(printer.stdout(), "Replaced existing pin with `{output}`")?;
    } else {
        writeln!(printer.stdout(), "Pinned to `{output}`")?;
    }

    Ok(ExitStatus::Success)
}
