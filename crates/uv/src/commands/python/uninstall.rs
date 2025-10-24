use std::collections::BTreeSet;
use std::fmt::Write;
use std::path::PathBuf;

use anyhow::Result;
use futures::StreamExt;
use futures::stream::FuturesUnordered;
use indexmap::IndexSet;
use itertools::Itertools;
use owo_colors::OwoColorize;
use rustc_hash::{FxHashMap, FxHashSet};
use tracing::{debug, warn};

use uv_fs::Simplified;
use uv_preview::Preview;
use uv_python::downloads::PythonDownloadRequest;
use uv_python::managed::{
    ManagedPythonInstallations, PythonMinorVersionLink, python_executable_dir,
};
use uv_python::{PythonInstallationKey, PythonInstallationMinorVersionKey, PythonRequest};

use crate::commands::python::install::format_executables;
use crate::commands::python::{ChangeEvent, ChangeEventKind};
use crate::commands::{ExitStatus, elapsed};
use crate::printer::Printer;

/// Uninstall managed Python versions.
pub(crate) async fn uninstall(
    install_dir: Option<PathBuf>,
    targets: Vec<String>,
    all: bool,
    outdated: bool,
    printer: Printer,
    preview: Preview,
) -> Result<ExitStatus> {
    let installations = ManagedPythonInstallations::from_settings(install_dir)?.init()?;

    let _lock = installations.lock().await?;

    // Perform the uninstallation.
    do_uninstall(&installations, targets, all, outdated, printer, preview).await?;

    // Clean up any empty directories.
    if uv_fs::directories(installations.root())?.all(|path| uv_fs::is_temporary(&path)) {
        fs_err::tokio::remove_dir_all(&installations.root()).await?;

        if let Some(top_level) = installations.root().parent() {
            // Remove the `toolchains` symlink.
            match fs_err::tokio::remove_file(top_level.join("toolchains")).await {
                Ok(()) => {}
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                Err(err) => return Err(err.into()),
            }

            if uv_fs::directories(top_level)?.all(|path| uv_fs::is_temporary(&path)) {
                fs_err::tokio::remove_dir_all(top_level).await?;
            }
        }
    }

    Ok(ExitStatus::Success)
}

fn requests_from_targets<'a>(
    targets: impl Iterator<Item = &'a str>,
    all: bool,
    outdated: bool,
) -> Result<Vec<PythonRequest>> {
    if all {
        return Ok(vec![PythonRequest::Default]);
    }

    let targets = targets.collect::<BTreeSet<_>>();
    let requests = targets
        .iter()
        .map(|target| PythonRequest::parse(target))
        .collect::<Vec<_>>();

    if requests.is_empty() {
        if !outdated {
            anyhow::bail!("No targets specified for uninstall");
        }
        return Ok(vec![PythonRequest::Default]);
    }

    Ok(requests)
}

/// Perform the uninstallation of managed Python installations.
async fn do_uninstall(
    installations: &ManagedPythonInstallations,
    targets: Vec<String>,
    all: bool,
    outdated: bool,
    printer: Printer,
    preview: Preview,
) -> Result<ExitStatus> {
    let start = std::time::Instant::now();

    let requests = requests_from_targets(targets.iter().map(String::as_str), all, outdated)?;

    let download_requests = requests
        .iter()
        .map(|request| {
            PythonDownloadRequest::from_request(request).ok_or_else(|| {
                anyhow::anyhow!("Cannot uninstall managed Python for request: {request}")
            })
        })
        // Always include pre-releases in uninstalls
        .map(|result| result.map(|request| request.with_prereleases(true)))
        .collect::<Result<Vec<_>>>()?;

    let installed_installations: Vec<_> = installations.find_all()?.collect();
    let latest_minor_installations = outdated.then(|| {
        PythonInstallationMinorVersionKey::highest_installations_by_minor_version_key(
            installed_installations.iter(),
        )
        .into_values()
        .map(|installation| installation.key().clone())
        .collect::<BTreeSet<_>>()
    });

    let mut matching_installations = BTreeSet::default();
    for (request, download_request) in requests.iter().zip(download_requests) {
        if matches!(requests.as_slice(), [PythonRequest::Default]) {
            writeln!(printer.stderr(), "Searching for Python installations")?;
        } else {
            writeln!(
                printer.stderr(),
                "Searching for Python versions matching: {}",
                request.cyan()
            )?;
        }

        // Check if this is a specific patch version request
        let is_specific_patch = download_request
            .version()
            .map(|v| matches!(v, uv_python::VersionRequest::MajorMinorPatch(..)))
            .unwrap_or(false);

        let mut found = false;
        for installation in installed_installations
            .iter()
            .filter(|installation| download_request.satisfied_by_key(installation.key()))
            .filter(|installation| {
                // When doing an outdated check, don't consider the latest of each minor version
                // as matching. However, if a specific patch version is requested, ignore the
                // outdated filter and uninstall the exact version requested.
                if is_specific_patch {
                    true
                } else {
                    latest_minor_installations
                        .as_ref()
                        .map(|latest_minor_installations| {
                            !latest_minor_installations.contains(installation.key())
                        })
                        .unwrap_or(true)
                }
            })
        {
            found = true;
            matching_installations.insert(installation.clone());
        }
        if !found {
            // Clear any remnants in the registry
            #[cfg(windows)]
            {
                uv_python::windows_registry::remove_orphan_registry_entries(
                    &installed_installations,
                );
            }

            if matches!(requests.as_slice(), [PythonRequest::Default]) {
                writeln!(printer.stderr(), "No Python installations found")?;
                return Ok(ExitStatus::Failure);
            }

            writeln!(
                printer.stderr(),
                "No existing installations found for: {}",
                request.cyan()
            )?;
        }
    }

    if matching_installations.is_empty() {
        let matching = if targets.is_empty() {
            ""
        } else {
            " matching the requests"
        };
        if outdated {
            writeln!(
                printer.stderr(),
                "No outdated Python installations found{matching}"
            )?;
            return Ok(ExitStatus::Success);
        }
        writeln!(printer.stderr(), "No Python installations found{matching}")?;
        return Ok(ExitStatus::Failure);
    }

    // Remove registry entries first, so we don't have dangling entries between the file removal
    // and the registry removal.
    let mut errors = vec![];
    #[cfg(windows)]
    {
        uv_python::windows_registry::remove_registry_entry(
            &matching_installations,
            all,
            &mut errors,
        );
        uv_python::windows_registry::remove_orphan_registry_entries(&installed_installations);
    }

    // Find and remove all relevant Python executables
    let mut uninstalled_executables: FxHashMap<PythonInstallationKey, FxHashSet<PathBuf>> =
        FxHashMap::default();
    for executable in python_executable_dir()?
        .read_dir()
        .into_iter()
        .flatten()
        .filter_map(|entry| match entry {
            Ok(entry) => Some(entry),
            Err(err) => {
                warn!("Failed to read executable: {}", err);
                None
            }
        })
        .filter(|entry| entry.file_type().is_ok_and(|file_type| !file_type.is_dir()))
        .map(|entry| entry.path())
        // Only include files that match the expected Python executable names
        // TODO(zanieb): This is a minor optimization to avoid opening more files, but we could
        // leave broken links behind, i.e., if the user created them.
        .filter(|path| {
            matching_installations.iter().any(|installation| {
                let name = path.file_name().and_then(|name| name.to_str());
                name == Some(&installation.key().executable_name_minor())
                    || name == Some(&installation.key().executable_name_major())
                    || name == Some(&installation.key().executable_name())
            })
        })
        .sorted()
    {
        let Some(installation) = matching_installations
            .iter()
            .find(|installation| installation.is_bin_link(executable.as_path()))
        else {
            continue;
        };

        fs_err::remove_file(&executable)?;
        debug!(
            "Removed `{}` for `{}`",
            executable.simplified_display(),
            installation.key()
        );
        uninstalled_executables
            .entry(installation.key().clone())
            .or_default()
            .insert(executable);
    }

    let mut tasks = FuturesUnordered::new();
    for installation in &matching_installations {
        tasks.push(async {
            (
                installation.key(),
                fs_err::tokio::remove_dir_all(installation.path()).await,
            )
        });
    }

    let mut uninstalled = IndexSet::<PythonInstallationKey>::default();
    while let Some((key, result)) = tasks.next().await {
        if let Err(err) = result {
            errors.push((key.clone(), anyhow::Error::new(err)));
        } else {
            uninstalled.insert(key.clone());
        }
    }

    // Read all existing managed installations and find the highest installed patch
    // for each installed minor version. Ensure the minor version link directory
    // is still valid.
    let uninstalled_minor_versions: IndexSet<_> = uninstalled
        .iter()
        .map(PythonInstallationMinorVersionKey::ref_cast)
        .collect();
    let remaining_installations: Vec<_> = installed_installations
        .into_iter()
        .filter(|installation| !uninstalled.contains(installation.key()))
        .collect();

    let remaining_minor_versions =
        PythonInstallationMinorVersionKey::highest_installations_by_minor_version_key(
            remaining_installations.iter(),
        );

    for (_, installation) in remaining_minor_versions
        .iter()
        .filter(|(minor_version, _)| uninstalled_minor_versions.contains(minor_version))
    {
        installation.update_minor_version_link(preview)?;
    }
    // For each uninstalled installation, check if there are no remaining installations
    // for its minor version. If there are none remaining, remove the symlink directory
    // (or junction on Windows) if it exists.
    for installation in &matching_installations {
        if !remaining_minor_versions.contains_key(installation.minor_version_key()) {
            if let Some(minor_version_link) =
                PythonMinorVersionLink::from_installation(installation, preview)
            {
                if minor_version_link.exists() {
                    let result = if cfg!(windows) {
                        fs_err::remove_dir(minor_version_link.symlink_directory.as_path())
                    } else {
                        fs_err::remove_file(minor_version_link.symlink_directory.as_path())
                    };
                    if result.is_err() {
                        return Err(anyhow::anyhow!(
                            "Failed to remove symlink directory {}",
                            minor_version_link.symlink_directory.display()
                        ));
                    }
                    let symlink_term = if cfg!(windows) {
                        "junction"
                    } else {
                        "symlink directory"
                    };
                    debug!(
                        "Removed {}: {}",
                        symlink_term,
                        minor_version_link.symlink_directory.to_string_lossy()
                    );
                }
            }
        }
    }

    // Report on any uninstalled installations.
    if let Some(first_uninstalled) = uninstalled.first() {
        if uninstalled.len() == 1 {
            // Ex) "Uninstalled Python 3.9.7 in 1.68s"
            writeln!(
                printer.stderr(),
                "{}",
                format!(
                    "Uninstalled {}{} {}",
                    if outdated { "outdated version " } else { "" },
                    format!("Python {}", first_uninstalled.version()).bold(),
                    format!("in {}", elapsed(start.elapsed())).dimmed()
                )
                .dimmed()
            )?;
        } else {
            // Ex) "Uninstalled 2 versions in 1.68s"
            writeln!(
                printer.stderr(),
                "{}",
                format!(
                    "Uninstalled {} {}",
                    format!(
                        "{} {}versions",
                        uninstalled.len(),
                        if outdated { "outdated " } else { "" }
                    )
                    .bold(),
                    format!("in {}", elapsed(start.elapsed())).dimmed()
                )
                .dimmed()
            )?;
        }

        for event in uninstalled
            .into_iter()
            .map(|key| ChangeEvent {
                key,
                kind: ChangeEventKind::Removed,
            })
            .sorted_unstable_by(|a, b| a.key.cmp(&b.key).then_with(|| a.kind.cmp(&b.kind)))
        {
            let executables = format_executables(&event, &uninstalled_executables);
            match event.kind {
                ChangeEventKind::Removed => {
                    writeln!(
                        printer.stderr(),
                        " {} {}{}",
                        "-".red(),
                        event.key.bold(),
                        executables,
                    )?;
                }
                _ => unreachable!(),
            }
        }
    }

    if !errors.is_empty() {
        for (key, err) in errors {
            writeln!(
                printer.stderr(),
                "Failed to uninstall {}: {}",
                key.green(),
                err.to_string().trim()
            )?;
        }
        return Ok(ExitStatus::Failure);
    }

    Ok(ExitStatus::Success)
}
