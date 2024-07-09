use std::collections::BTreeSet;
use std::fmt::Write;

use anyhow::Result;
use futures::StreamExt;
use itertools::Itertools;
use owo_colors::OwoColorize;

use uv_configuration::PreviewMode;
use uv_python::downloads::{self, PythonDownloadRequest};
use uv_python::managed::ManagedPythonInstallations;
use uv_python::PythonRequest;
use uv_warnings::warn_user_once;

use crate::commands::{elapsed, ExitStatus};
use crate::printer::Printer;

/// Uninstall managed Python versions.
pub(crate) async fn uninstall(
    targets: Vec<String>,
    all: bool,
    preview: PreviewMode,
    printer: Printer,
) -> Result<ExitStatus> {
    if preview.is_disabled() {
        warn_user_once!("`uv python uninstall` is experimental and may change without warning.");
    }

    let start = std::time::Instant::now();

    let installations = ManagedPythonInstallations::from_settings()?.init()?;
    let _lock = installations.acquire_lock()?;

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
        .map(PythonDownloadRequest::from_request)
        .collect::<Result<Vec<_>, downloads::Error>>()?;

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
            if matching_installations.insert(installation.clone()) {
                if matches!(requests.as_slice(), [PythonRequest::Any]) {
                    writeln!(printer.stderr(), "Found: {}", installation.key().green(),)?;
                } else {
                    writeln!(
                        printer.stderr(),
                        "Found existing installation for {}: {}",
                        request.cyan(),
                        installation.key().green(),
                    )?;
                }
            }
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

    let tasks = futures::stream::iter(matching_installations.iter())
        .map(|installation| async {
            (
                installation.key(),
                fs_err::tokio::remove_dir_all(installation.path()).await,
            )
        })
        .buffered(4);

    let results = tasks.collect::<Vec<_>>().await;
    let mut failed = false;
    for (key, result) in results.iter().sorted_by_key(|(key, _)| key) {
        if let Err(err) = result {
            failed = true;
            writeln!(
                printer.stderr(),
                "Failed to uninstall {}: {err}",
                key.green()
            )?;
        } else {
            writeln!(printer.stderr(), "Uninstalled: {}", key.green())?;
        }
    }

    if failed {
        if matching_installations.len() > 1 {
            writeln!(printer.stderr(), "Failed to uninstall some Python versions")?;
        }
        return Ok(ExitStatus::Failure);
    }

    let s = if matching_installations.len() == 1 {
        ""
    } else {
        "s"
    };
    writeln!(
        printer.stderr(),
        "{}",
        format!(
            "Uninstalled {} {}",
            format!("{} version{s}", matching_installations.len()).bold(),
            format!("in {}", elapsed(start.elapsed())).dimmed()
        )
        .dimmed()
    )?;

    Ok(ExitStatus::Success)
}
