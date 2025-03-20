use std::collections::BTreeSet;
use std::fmt::Write;
use std::path::PathBuf;

use anyhow::Result;
use futures::StreamExt;
use futures::stream::FuturesUnordered;
use itertools::Itertools;
use owo_colors::OwoColorize;
use rustc_hash::{FxHashMap, FxHashSet};
use tracing::{debug, warn};

use uv_configuration::PreviewMode;
use uv_fs::Simplified;
use uv_python::downloads::PythonDownloadRequest;
use uv_python::managed::{ManagedPythonInstallations, python_executable_dir, symlink_exists};
use uv_python::{PythonInstallationKey, PythonRequest};

use crate::commands::python::install::format_executables;
use crate::commands::python::{ChangeEvent, ChangeEventKind};
use crate::commands::{ExitStatus, elapsed};
use crate::printer::Printer;

/// Uninstall managed Python versions.
pub(crate) async fn uninstall(
    install_dir: Option<PathBuf>,
    targets: Vec<String>,
    all: bool,
    printer: Printer,
    preview: PreviewMode,
) -> Result<ExitStatus> {
    let installations = ManagedPythonInstallations::from_settings(install_dir)?.init()?;

    let _lock = installations.lock().await?;

    // Perform the uninstallation.
    do_uninstall(&installations, targets, all, printer, preview).await?;

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

/// Perform the uninstallation of managed Python installations.
async fn do_uninstall(
    installations: &ManagedPythonInstallations,
    targets: Vec<String>,
    all: bool,
    printer: Printer,
    preview: PreviewMode,
) -> Result<ExitStatus> {
    let start = std::time::Instant::now();

    let requests = if all {
        vec![PythonRequest::Default]
    } else {
        let targets = targets.into_iter().collect::<BTreeSet<_>>();
        targets
            .iter()
            .map(|target| PythonRequest::parse(target.as_str()))
            .collect::<Vec<_>>()
    };

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
        let mut found = false;
        for installation in installed_installations
            .iter()
            .filter(|installation| download_request.satisfied_by_key(installation.key()))
        {
            found = true;
            matching_installations.insert(installation.clone());
        }
        if !found {
            // Clear any remnants in the registry
            if preview.is_enabled() {
                #[cfg(windows)]
                {
                    uv_python::windows_registry::remove_orphan_registry_entries(
                        &installed_installations,
                    );
                }
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
        writeln!(
            printer.stderr(),
            "No Python installations found matching the requests"
        )?;
        return Ok(ExitStatus::Failure);
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

    let mut uninstalled = vec![];
    let mut errors = vec![];
    while let Some((key, result)) = tasks.next().await {
        if let Err(err) = result {
            errors.push((key.clone(), anyhow::Error::new(err)));
        } else {
            uninstalled.push(key.clone());
        }
    }

    #[cfg(windows)]
    if preview.is_enabled() {
        uv_python::windows_registry::remove_registry_entry(
            &matching_installations,
            all,
            &mut errors,
        );
        uv_python::windows_registry::remove_orphan_registry_entries(&installed_installations);
    }

    // Read all existing installations and find the highest installed patch
    // for each installed minor version. Ensure the minor version link directory
    // is still valid.
    let uninstalled_minor_versions =
        &uninstalled
            .iter()
            .fold(FxHashSet::default(), |mut minor_versions, key| {
                minor_versions.insert(key.version().python_version());
                minor_versions
            });
    let installations = ManagedPythonInstallations::from_settings(None)?.init()?;
    let remaining_installations: Vec<_> = installations.find_all()?.collect();
    let mut remaining_minor_versions = FxHashMap::default();
    for installation in remaining_installations {
        // Add to minor versions map if this installation has the highest
        // patch seen for a minor version so far.
        let minor_version = installation.version().python_version();
        if !uninstalled_minor_versions.contains(&minor_version) {
            continue;
        }
        if let Some(patch) = installation.version().patch() {
            if let Some((current_patch, _)) = remaining_minor_versions.get(&minor_version) {
                if patch >= *current_patch {
                    remaining_minor_versions.insert(minor_version, (patch, installation));
                }
            } else {
                remaining_minor_versions.insert(minor_version, (patch, installation));
            }
        }
    }
    for (_, installation) in remaining_minor_versions.values() {
        installation.ensure_minor_version_link()?;
    }
    // For each uninstalled installation, check if there are no remaining installations
    // for its minor version. If there are none remaining, remove the symlink directory
    // (or junction on Windows) if it exists.
    for installation in &matching_installations {
        if !remaining_minor_versions.contains_key(&installation.key().version().python_version()) {
            let symlink_directory_name = format!(
                "python{}.{}",
                installation.key().version().major(),
                installation.key().version().minor()
            );
            // Derive a symbolic directory path from the installation home path by replacing the
            // home directory name with the symlink directory name.
            let symlink_directory = installation.path().with_file_name(&symlink_directory_name);
            if symlink_exists(symlink_directory.as_path()) {
                let result = if cfg!(windows) {
                    fs_err::remove_dir(symlink_directory.as_path())
                } else {
                    fs_err::remove_file(symlink_directory.as_path())
                };
                if result.is_err() {
                    return Err(anyhow::anyhow!(
                        "Failed to remove symlink directory {}",
                        symlink_directory.display()
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
                    symlink_directory.to_string_lossy()
                );
            }
        }
    }

    // Report on any uninstalled installations.
    if !uninstalled.is_empty() {
        if let [uninstalled] = uninstalled.as_slice() {
            // Ex) "Uninstalled Python 3.9.7 in 1.68s"
            writeln!(
                printer.stderr(),
                "{}",
                format!(
                    "Uninstalled {} {}",
                    format!("Python {}", uninstalled.version()).bold(),
                    format!("in {}", elapsed(start.elapsed())).dimmed()
                )
                .dimmed()
            )?;
        } else {
            // Ex) "Uninstalled 2 versions in 1.68s"
            let s = if uninstalled.len() == 1 { "" } else { "s" };
            writeln!(
                printer.stderr(),
                "{}",
                format!(
                    "Uninstalled {} {}",
                    format!("{} version{s}", uninstalled.len()).bold(),
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
