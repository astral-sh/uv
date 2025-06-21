use std::fmt::Write;
use std::path::Path;
use std::str::FromStr;

use anyhow::{Result, bail};
use owo_colors::OwoColorize;
use tracing::debug;

use uv_cache::Cache;
use uv_client::BaseClientBuilder;
use uv_configuration::{DependencyGroupsWithDefaults, PreviewMode};
use uv_fs::Simplified;
use uv_python::{
    EnvironmentPreference, PYTHON_VERSION_FILENAME, PythonDownloads, PythonInstallation,
    PythonPreference, PythonRequest, PythonVersionFile, VersionFileDiscoveryOptions,
};
use uv_settings::PythonInstallMirrors;
use uv_warnings::warn_user_once;
use uv_workspace::{DiscoveryOptions, VirtualProject, WorkspaceCache};

use crate::commands::{
    ExitStatus, project::find_requires_python, reporters::PythonDownloadReporter,
};
use crate::printer::Printer;
use crate::settings::NetworkSettings;

/// Pin to a specific Python version.
#[allow(clippy::fn_params_excessive_bools)]
pub(crate) async fn pin(
    project_dir: &Path,
    request: Option<String>,
    resolved: bool,
    python_preference: PythonPreference,
    python_downloads: PythonDownloads,
    no_project: bool,
    global: bool,
    rm: bool,
    install_mirrors: PythonInstallMirrors,
    network_settings: NetworkSettings,
    cache: &Cache,
    printer: Printer,
    preview: PreviewMode,
) -> Result<ExitStatus> {
    let workspace_cache = WorkspaceCache::default();
    let virtual_project = if no_project {
        None
    } else {
        match VirtualProject::discover(project_dir, &DiscoveryOptions::default(), &workspace_cache)
            .await
        {
            Ok(virtual_project) => Some(virtual_project),
            Err(err) => {
                debug!("Failed to discover virtual project: {err}");
                None
            }
        }
    };

    // Search for an existing file, we won't necessarily write to this, we'll construct a target
    // path if there's a request later on.
    let version_file = PythonVersionFile::discover(
        project_dir,
        &VersionFileDiscoveryOptions::default().with_no_local(global),
    )
    .await;

    if rm {
        let Some(file) = version_file? else {
            if global {
                bail!("No global Python pin found");
            }
            bail!("No Python version file found");
        };

        if !global
            && PythonVersionFile::new_global().is_some_and(|global| file.path() == global.path())
        {
            bail!("No Python version file found; use `--rm --global` to remove the global pin");
        }

        fs_err::tokio::remove_file(file.path()).await?;
        writeln!(
            printer.stdout(),
            "Removed {} at `{}`",
            if global {
                "global Python pin"
            } else {
                "Python version file"
            },
            file.path().user_display()
        )?;
        return Ok(ExitStatus::Success);
    }

    let Some(request) = request else {
        // Display the current pinned Python version
        if let Some(file) = version_file? {
            for pin in file.versions() {
                writeln!(printer.stdout(), "{}", pin.to_canonical_string())?;
                if let Some(virtual_project) = &virtual_project {
                    warn_if_existing_pin_incompatible_with_project(
                        pin,
                        virtual_project,
                        python_preference,
                        cache,
                        preview,
                    );
                }
            }
            return Ok(ExitStatus::Success);
        }
        bail!("No Python version file found; specify a version to create one")
    };
    let request = PythonRequest::parse(&request);

    if let PythonRequest::ExecutableName(name) = request {
        bail!("Requests for arbitrary names (e.g., `{name}`) are not supported in version files");
    }

    let client_builder = BaseClientBuilder::new()
        .connectivity(network_settings.connectivity)
        .native_tls(network_settings.native_tls)
        .allow_insecure_host(network_settings.allow_insecure_host.clone());
    let reporter = PythonDownloadReporter::single(printer);

    let python = match PythonInstallation::find_or_download(
        Some(&request),
        EnvironmentPreference::OnlySystem,
        python_preference,
        python_downloads,
        &client_builder,
        cache,
        Some(&reporter),
        install_mirrors.python_install_mirror.as_deref(),
        install_mirrors.pypy_install_mirror.as_deref(),
        install_mirrors.python_downloads_json_url.as_deref(),
        preview,
    )
    .await
    {
        Ok(python) => Some(python),
        // If no matching Python version is found, don't fail unless `resolved` was requested
        Err(uv_python::Error::MissingPython(err)) if !resolved => {
            warn_user_once!("{err}");
            None
        }
        // If there was some other error, log it
        Err(err) if !resolved => {
            debug!("{err}");
            None
        }
        // If `resolved` was requested, we must find an interpreter — fail otherwise
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
                    }
                    warn_user_once!("{err}");
                }
            }
        }
    }

    let request = if resolved {
        // SAFETY: We exit early if Python is not found and resolved is `true`
        // TODO(zanieb): Maybe avoid reparsing here?
        PythonRequest::parse(
            &python
                .unwrap()
                .interpreter()
                .sys_executable()
                .user_display()
                .to_string(),
        )
    } else {
        request
    };

    let existing = version_file.ok().flatten();
    // TODO(zanieb): Allow updating the discovered version file with an `--update` flag.
    let new = if global {
        let Some(new) = PythonVersionFile::new_global() else {
            // TODO(zanieb): We should find a nice way to surface that as an error
            bail!("Failed to determine directory for global Python pin");
        };
        new.with_versions(vec![request])
    } else {
        PythonVersionFile::new(project_dir.join(PYTHON_VERSION_FILENAME))
            .with_versions(vec![request])
    };

    new.write().await?;

    // If we updated an existing version file to a new version
    if let Some(existing) = existing
        .as_ref()
        .filter(|existing| existing.path() == new.path())
        .and_then(PythonVersionFile::version)
        .filter(|version| *version != new.version().unwrap())
    {
        writeln!(
            printer.stdout(),
            "Updated `{}` from `{}` -> `{}`",
            new.path().user_display().cyan(),
            existing.to_canonical_string().green(),
            new.version().unwrap().to_canonical_string().green()
        )?;
    } else {
        writeln!(
            printer.stdout(),
            "Pinned `{}` to `{}`",
            new.path().user_display().cyan(),
            new.version().unwrap().to_canonical_string().green()
        )?;
    }

    Ok(ExitStatus::Success)
}

fn pep440_version_from_request(request: &PythonRequest) -> Option<uv_pep440::Version> {
    let version_request = match request {
        PythonRequest::Version(version) | PythonRequest::ImplementationVersion(_, version) => {
            version
        }
        PythonRequest::Key(download_request) => download_request.version()?,
        _ => {
            return None;
        }
    };

    if matches!(version_request, uv_python::VersionRequest::Range(_, _)) {
        return None;
    }

    // SAFETY: converting `VersionRequest` to `Version` is guaranteed to succeed if not a `Range`
    // and does not have a Python variant (e.g., freethreaded) attached.
    Some(
        uv_pep440::Version::from_str(&version_request.clone().without_python_variant().to_string())
            .unwrap(),
    )
}

/// Check if pinned request is compatible with the workspace/project's `Requires-Python`.
fn warn_if_existing_pin_incompatible_with_project(
    pin: &PythonRequest,
    virtual_project: &VirtualProject,
    python_preference: PythonPreference,
    cache: &Cache,
    preview: PreviewMode,
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
            warn_user_once!("{err}");
            return;
        }
    }

    // If there is not a version in the pinned request, attempt to resolve the pin into an
    // interpreter to check for compatibility on the current system.
    match PythonInstallation::find(
        pin,
        EnvironmentPreference::OnlySystem,
        python_preference,
        cache,
        preview,
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
                warn_user_once!("{err}");
            }
        }
        Err(err) => {
            warn_user_once!(
                "Failed to resolve pinned Python version `{}`: {err}",
                pin.to_canonical_string(),
            );
        }
    }
}

/// Utility struct for representing pins in error messages.
struct Pin<'a> {
    request: &'a PythonRequest,
    version: &'a uv_pep440::Version,
    resolved: bool,
    existing: bool,
}

/// Checks if the pinned Python version is compatible with the workspace/project's `Requires-Python`.
fn assert_pin_compatible_with_project(pin: &Pin, virtual_project: &VirtualProject) -> Result<()> {
    // Don't factor in requires-python settings on dependency-groups
    let groups = DependencyGroupsWithDefaults::none();

    let (requires_python, project_type) = match virtual_project {
        VirtualProject::Project(project_workspace) => {
            debug!(
                "Discovered project `{}` at: {}",
                project_workspace.project_name(),
                project_workspace.workspace().install_path().display()
            );

            let requires_python = find_requires_python(project_workspace.workspace(), &groups)?;
            (requires_python, "project")
        }
        VirtualProject::NonProject(workspace) => {
            debug!(
                "Discovered virtual workspace at: {}",
                workspace.install_path().display()
            );
            let requires_python = find_requires_python(workspace, &groups)?;
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
