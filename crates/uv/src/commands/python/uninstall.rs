use std::collections::BTreeSet;
use std::fmt::Write;

use anyhow::Result;
use futures::stream::FuturesUnordered;
use futures::StreamExt;
use itertools::Itertools;
use owo_colors::OwoColorize;

use uv_python::downloads::PythonDownloadRequest;
use uv_python::managed::ManagedPythonInstallations;
use uv_python::PythonRequest;

use crate::commands::python::{ChangeEvent, ChangeEventKind};
use crate::commands::{elapsed, ExitStatus};
use crate::printer::Printer;

/// Uninstall managed Python versions.
pub(crate) async fn uninstall(
    targets: Vec<String>,
    all: bool,

    printer: Printer,
) -> Result<ExitStatus> {
    let installations = ManagedPythonInstallations::from_settings()?.init()?;
    let _lock = installations.lock().await?;

    // Perform the uninstallation.
    do_uninstall(&installations, targets, all, printer).await?;

    // Clean up any empty directories.
    if uv_fs::directories(installations.root()).all(|path| uv_fs::is_temporary(&path)) {
        fs_err::tokio::remove_dir_all(&installations.root()).await?;

        if let Some(top_level) = installations.root().parent() {
            // Remove the `toolchains` symlink.
            match uv_fs::remove_symlink(top_level.join("toolchains")) {
                Ok(()) => {}
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                Err(err) => return Err(err.into()),
            }

            if uv_fs::directories(top_level).all(|path| uv_fs::is_temporary(&path)) {
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
) -> Result<ExitStatus> {
    let start = std::time::Instant::now();

    let requests = if all {
        vec![PythonRequest::Any]
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
        if matches!(requests.as_slice(), [PythonRequest::Any]) {
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
            if matches!(requests.as_slice(), [PythonRequest::Any]) {
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
            errors.push((key.clone(), err));
        } else {
            uninstalled.push(key.clone());
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
            match event.kind {
                ChangeEventKind::Added => {
                    writeln!(printer.stderr(), " {} {}", "+".green(), event.key.bold())?;
                }
                ChangeEventKind::Removed => {
                    writeln!(printer.stderr(), " {} {}", "-".red(), event.key.bold())?;
                }
            }
        }
    }

    if !errors.is_empty() {
        for (key, err) in errors {
            writeln!(
                printer.stderr(),
                "Failed to uninstall {}: {}",
                key.green(),
                err
            )?;
        }
        return Ok(ExitStatus::Failure);
    }

    Ok(ExitStatus::Success)
}
