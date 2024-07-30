use std::fmt::Write;
use std::str::FromStr;

use anyhow::{bail, Result};
use owo_colors::OwoColorize;
use tracing::debug;

use uv_cache::Cache;
use uv_configuration::PreviewMode;
use uv_fs::{Simplified, CWD};
use uv_python::{
    request_from_version_file, requests_from_version_file, write_version_file,
    EnvironmentPreference, PythonInstallation, PythonPreference, PythonRequest,
    PYTHON_VERSION_FILENAME,
};
use uv_warnings::warn_user_once;
use uv_workspace::{DiscoveryOptions, VirtualProject};

use crate::commands::{project::find_requires_python, ExitStatus};
use crate::printer::Printer;

/// Pin to a specific Python version.
pub(crate) async fn pin(
    request: Option<String>,
    resolved: bool,
    python_preference: PythonPreference,
    preview: PreviewMode,
    no_workspace: bool,
    cache: &Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    if preview.is_disabled() {
        warn_user_once!("`uv python pin` is experimental and may change without warning");
    }

    let virtual_project = if no_workspace {
        None
    } else {
        match VirtualProject::discover(&CWD, &DiscoveryOptions::default()).await {
            Ok(virtual_project) => Some(virtual_project),
            Err(err) => {
                debug!("Failed to discover virtual project: {err}");
                None
            }
        }
    };

    let Some(request) = request else {
        // Display the current pinned Python version
        if let Some(pins) = requests_from_version_file(&CWD).await? {
            for pin in pins {
                writeln!(printer.stdout(), "{}", pin.to_canonical_string())?;
                if let Some(virtual_project) = &virtual_project {
                    warn_if_existing_pin_incompatible_with_project(
                        &pin,
                        virtual_project,
                        python_preference,
                        cache,
                    );
                }
            }
            return Ok(ExitStatus::Success);
        }
        bail!("No pinned Python version found")
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
            warn_user_once!("{err}");
            None
        }
        Err(err) => return Err(err.into()),
    };

    if let Some(virtual_project) = &virtual_project {
        if let Some(request_version) = pep440_version_from_request(&request) {
            assert_pin_compatible_with_project(
                &Pin {
                    request: &request,
                    version: &request_version,
                    resolved: false,
                    existing: false,
                },
                virtual_project,
            )?;
        } else {
            if let Some(python) = &python {
                // Warn if the resolved Python is incompatible with the Python requirement unless --resolved is used
                if let Err(err) = assert_pin_compatible_with_project(
                    &Pin {
                        request: &request,
                        version: python.python_version(),
                        resolved: true,
                        existing: false,
                    },
                    virtual_project,
                ) {
                    if resolved {
                        return Err(err);
                    };
                    warn_user_once!("{}", err);
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

    let existing = request_from_version_file(&CWD).await.ok().flatten();
    write_version_file(&output).await?;

    if let Some(existing) = existing
        .map(|existing| existing.to_canonical_string())
        .filter(|existing| existing != &output)
    {
        writeln!(
            printer.stdout(),
            "Updated `{}` from `{}` -> `{}`",
            PYTHON_VERSION_FILENAME.cyan(),
            existing.green(),
            output.green()
        )?;
    } else {
        writeln!(
            printer.stdout(),
            "Pinned `{}` to `{}`",
            PYTHON_VERSION_FILENAME.cyan(),
            output.green()
        )?;
    }

    Ok(ExitStatus::Success)
}

fn pep440_version_from_request(request: &PythonRequest) -> Option<pep440_rs::Version> {
    let version_request = match request {
        PythonRequest::Version(ref version)
        | PythonRequest::ImplementationVersion(_, ref version) => version,
        PythonRequest::Key(download_request) => download_request.version()?,
        _ => {
            return None;
        }
    };

    if matches!(version_request, uv_python::VersionRequest::Range(_)) {
        return None;
    }

    // SAFETY: converting `VersionRequest` to `Version` is guaranteed to succeed if not a `Range`.
    Some(pep440_rs::Version::from_str(&version_request.to_string()).unwrap())
}

/// Check if pinned request is compatible with the workspace/project's `Requires-Python`.
fn warn_if_existing_pin_incompatible_with_project(
    pin: &PythonRequest,
    virtual_project: &VirtualProject,
    python_preference: PythonPreference,
    cache: &Cache,
) {
    // Check if the pinned version is compatible with the project.
    if let Some(pin_version) = pep440_version_from_request(pin) {
        if let Err(err) = assert_pin_compatible_with_project(
            &Pin {
                request: pin,
                version: &pin_version,
                resolved: false,
                existing: true,
            },
            virtual_project,
        ) {
            warn_user_once!("{}", err);
            return;
        }
    }

    // If the there is not a version in the pinned request, attempt to resolve the pin into an interpreter
    // to check for compatibility on the current system.
    match PythonInstallation::find(
        pin,
        EnvironmentPreference::OnlySystem,
        python_preference,
        cache,
    ) {
        Ok(python) => {
            let python_version = python.python_version();
            debug!(
                "The pinned Python version `{}` resolves to `{}`",
                pin, python_version
            );
            // Warn on incompatibilities when viewing existing pins
            if let Err(err) = assert_pin_compatible_with_project(
                &Pin {
                    request: pin,
                    version: python_version,
                    resolved: true,
                    existing: true,
                },
                virtual_project,
            ) {
                warn_user_once!("{}", err);
            }
        }
        Err(err) => {
            warn_user_once!(
                "Failed to resolve pinned Python version `{}`: {}",
                pin.to_canonical_string(),
                err
            );
        }
    }
}

/// Utility struct for representing pins in error messages.
struct Pin<'a> {
    request: &'a PythonRequest,
    version: &'a pep440_rs::Version,
    resolved: bool,
    existing: bool,
}

/// Checks if the pinned Python version is compatible with the workspace/project's `Requires-Python`.
fn assert_pin_compatible_with_project(pin: &Pin, virtual_project: &VirtualProject) -> Result<()> {
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

    let Some(requires_python) = requires_python else {
        return Ok(());
    };

    if requires_python.contains(pin.version) {
        return Ok(());
    }

    let given = if pin.existing { "pinned" } else { "requested" };
    let resolved = if pin.resolved {
        format!(" resolves to `{}` which ", pin.version)
    } else {
        String::new()
    };

    Err(anyhow::anyhow!(
        "The {given} Python version `{}`{resolved} is incompatible with the {} `requires-python` value of `{}`.",
        pin.request.to_canonical_string(),
        project_type,
        requires_python
    ))
}
