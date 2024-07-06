use std::collections::BTreeSet;
use std::fmt::Write;

use anyhow::Result;
use futures::StreamExt;
use itertools::Itertools;

use uv_configuration::PreviewMode;
use uv_python::downloads::{self, PythonDownloadRequest};
use uv_python::managed::ManagedPythonInstallations;
use uv_python::PythonRequest;
use uv_warnings::warn_user_once;

use crate::commands::ExitStatus;
use crate::printer::Printer;

/// Uninstall managed Python versions.
pub(crate) async fn uninstall(
    targets: Vec<String>,
    preview: PreviewMode,
    printer: Printer,
) -> Result<ExitStatus> {
    if preview.is_disabled() {
        warn_user_once!("`uv python uninstall` is experimental and may change without warning.");
    }

    let installations = ManagedPythonInstallations::from_settings()?.init()?;
    let _lock = installations.acquire_lock()?;

    let targets = targets.into_iter().collect::<BTreeSet<_>>();
    let requests = targets
        .iter()
        .map(|target| PythonRequest::parse(target.as_str()))
        .collect::<Vec<_>>();

    let download_requests = requests
        .iter()
        .map(PythonDownloadRequest::from_request)
        .collect::<Result<Vec<_>, downloads::Error>>()?;

    let installed_installations: Vec<_> = installations.find_all()?.collect();
    let mut matching_installations = BTreeSet::default();
    for (request, download_request) in requests.iter().zip(download_requests) {
        writeln!(
            printer.stderr(),
            "Looking for Python installations matching {request} ({download_request})"
        )?;
        let mut found = false;
        for installation in installed_installations
            .iter()
            .filter(|installation| download_request.satisfied_by_key(installation.key()))
        {
            found = true;
            if matching_installations.insert(installation.clone()) {
                writeln!(
                    printer.stderr(),
                    "Found installation `{}` that matches {request}",
                    installation.key()
                )?;
            }
        }
        if !found {
            writeln!(
                printer.stderr(),
                "No installations found matching {request}"
            )?;
        }
    }

    if matching_installations.is_empty() {
        if matches!(requests.as_slice(), [PythonRequest::Any]) {
            writeln!(printer.stderr(), "No installed installations found")?;
        } else if requests.len() > 1 {
            writeln!(
                printer.stderr(),
                "No installations found matching the requests"
            )?;
        } else {
            writeln!(
                printer.stderr(),
                "No installations found matching the request"
            )?;
        }
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
            writeln!(printer.stderr(), "Failed to uninstall `{key}`: {err}")?;
        } else {
            writeln!(printer.stderr(), "Uninstalled `{key}`")?;
        }
    }

    if failed {
        if matching_installations.len() > 1 {
            writeln!(
                printer.stderr(),
                "Failed to remove some Python installations"
            )?;
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
        "Removed {} Python installation{s}",
        matching_installations.len()
    )?;

    Ok(ExitStatus::Success)
}
