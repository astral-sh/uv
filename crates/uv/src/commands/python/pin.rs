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

    let virtual_project = VirtualProject::discover(&std::env::current_dir()?, None).await;

    let Some(request) = request else {
        // Display the current pinned Python version
        if let Some(pins) = requests_from_version_file().await? {
            for pin in pins {
                let python = match PythonInstallation::find(
                    &pin,
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
                    if let (Some(python), Ok(virtual_project)) = (&python, &virtual_project) {
                        if let Err(e) = assert_python_compatibility(python, virtual_project) {
                            warn_user_once!("{}", e);
                        }
                    }
                }
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
        if let (Some(python), Ok(virtual_project)) = (&python, &virtual_project) {
            assert_python_compatibility(python, virtual_project)?;
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

/// Checks if the pinned Python version is compatible with the workspace/project's `Requires-Python`.
fn assert_python_compatibility(
    python: &uv_python::PythonInstallation,
    virtual_project: &VirtualProject,
) -> Result<()> {
    let (requires_python, project_type) = match virtual_project {
        VirtualProject::Project(project_workspace) => {
            debug!(
                "Discovered project `{}` at: {}",
                project_workspace.project_name(),
                project_workspace.workspace().install_path().display()
            );
            let requires_python = find_requires_python(project_workspace.workspace())?;
            (requires_python, "project")
        }
        VirtualProject::Virtual(workspace) => {
            debug!(
                "Discovered virtual workspace at: {}",
                workspace.install_path().display()
            );
            let requires_python = find_requires_python(workspace)?;
            (requires_python, "workspace")
        }
    };

    if let Some(requires_python) = requires_python {
        if !requires_python.contains(python.python_version()) {
            anyhow::bail!(
                "The pinned Python version is incompatible with the {}'s `Requires-Python` of {}.",
                project_type,
                requires_python
            );
        }
    }
    Ok(())
}
