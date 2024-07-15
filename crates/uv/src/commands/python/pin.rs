use std::path::PathBuf;
use std::{fmt::Write, str::FromStr};

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

    let virtual_project = match VirtualProject::discover(&std::env::current_dir()?, None).await {
        Ok(virtual_project) if !isolated => Some(virtual_project),
        Ok(_) => None,
        Err(e) => {
            debug!("Failed to discover virtual project: {e}");
            None
        }
    };

    let Some(request) = request else {
        // Display the current pinned Python version
        if let Some(pins) = requests_from_version_file().await? {
            for pin in pins {
                writeln!(printer.stdout(), "{}", pin.to_canonical_string())?;
                if let Some(virtual_project) = &virtual_project {
                    check_request_requires_python_compatibility(
                        &pin,
                        virtual_project,
                        python_preference,
                        cache,
                    );
                }
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

    if let Some(virtual_project) = &virtual_project {
        // Error if the request is incompatible with the Python requirement
        if let PythonRequest::Version(version) = &request {
            if let Ok(python_version) = pep440_rs::Version::from_str(&version.to_string()) {
                assert_python_compatibility(&python_version, virtual_project)?;
            }
        } else {
            if let Some(python) = &python {
                // Warn if the resolved Python is incompatible with the Python requirement unless --resolved is used
                if let Err(e) =
                    assert_python_compatibility(python.python_version(), virtual_project)
                {
                    if resolved {
                        return Err(e);
                    };
                    warn_user_once!("{}", e);
                }
            }
        };
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

/// Check if pinned request is compatible with the workspace/project's `Requires-Python`.
fn check_request_requires_python_compatibility(
    pin: &PythonRequest,
    virtual_project: &VirtualProject,
    python_preference: PythonPreference,
    cache: &Cache,
) {
    let requested_version = match pin {
        PythonRequest::Version(ref version) => {
            let version = pep440_rs::Version::from_str(&version.to_string());
            match version {
                Ok(version) => Some(version),
                Err(e) => {
                    debug!("Failed to parse PEP440 python version from {pin}: {e}");
                    None
                }
            }
        }
        _ => None,
    };

    // Check if the requested version is compatible with the project.
    // If the compatibility check fails, exit early.
    if let Some(version) = requested_version {
        if let Err(e) = assert_python_compatibility(&version, virtual_project) {
            warn_user_once!("{}", e);
            return;
        }
    };

    // If the requested version is either not specified or compatible, attempt to resolve the request into an interpreter.
    let python_version = match PythonInstallation::find(
        pin,
        EnvironmentPreference::OnlySystem,
        python_preference,
        cache,
    ) {
        Ok(python) => Ok(python.python_version().clone()),
        Err(err) => Err(err.to_string()),
    };

    match python_version {
        Ok(python_version) => {
            debug!(
                "The pinned Python version {} resolves to {}",
                pin, python_version
            );
            if let Err(e) = assert_python_compatibility(&python_version, virtual_project) {
                warn_user_once!("{}", e);
            }
        }
        Err(e) => {
            warn_user_once!(
                "Failed to resolve pinned Python version from {}: {}",
                pin,
                e
            );
        }
    }
}

/// Checks if the pinned Python version is compatible with the workspace/project's `Requires-Python`.
fn assert_python_compatibility(
    python_version: &pep440_rs::Version,
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
        if !requires_python.contains(python_version) {
            anyhow::bail!(
                "The pinned Python version {} is incompatible with the {}'s `Requires-Python` of {}.",
                python_version,
                project_type,
                requires_python
            );
        }
    }
    Ok(())
}
