use std::collections::BTreeSet;
use std::fmt::Write;

use anyhow::Result;
use fs_err as fs;
use futures::StreamExt;
use itertools::Itertools;
use uv_cache::Cache;
use uv_client::Connectivity;
use uv_configuration::PreviewMode;
use uv_fs::Simplified;
use uv_python::downloads::{self, DownloadResult, ManagedPythonDownload, PythonDownloadRequest};
use uv_python::managed::{ManagedPythonInstallation, ManagedPythonInstallations};
use uv_python::{requests_from_version_file, PythonRequest};
use uv_warnings::warn_user_once;

use crate::commands::ExitStatus;
use crate::printer::Printer;

/// Download and install Python versions.
pub(crate) async fn install(
    targets: Vec<String>,
    force: bool,
    native_tls: bool,
    connectivity: Connectivity,
    preview: PreviewMode,
    _cache: &Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    if preview.is_disabled() {
        warn_user_once!("`uv python install` is experimental and may change without warning.");
    }

    let start = std::time::Instant::now();

    let installations = ManagedPythonInstallations::from_settings()?.init()?;
    let installations_dir = installations.root();
    let _lock = installations.acquire_lock()?;

    let targets = targets.into_iter().collect::<BTreeSet<_>>();
    let requests: Vec<_> = if targets.is_empty() {
        if let Some(requests) = requests_from_version_file().await? {
            requests
        } else {
            vec![PythonRequest::Any]
        }
    } else {
        targets
            .iter()
            .map(|target| PythonRequest::parse(target.as_str()))
            .collect()
    };

    let download_requests = requests
        .iter()
        .map(PythonDownloadRequest::from_request)
        .collect::<Result<Vec<_>, downloads::Error>>()?;

    let installed_installations: Vec<_> = installations.find_all()?.collect();
    let mut unfilled_requests = Vec::new();
    for (request, download_request) in requests.iter().zip(download_requests) {
        writeln!(
            printer.stderr(),
            "Looking for installation {request} ({download_request})"
        )?;
        if let Some(installation) = installed_installations
            .iter()
            .find(|installation| download_request.satisfied_by_key(installation.key()))
        {
            writeln!(
                printer.stderr(),
                "Found existing installation `{}` that satisfies {request}",
                installation.key()
            )?;
            if force {
                writeln!(
                    printer.stderr(),
                    "Removing existing installation `{}`",
                    installation.key()
                )?;
                fs::remove_dir_all(installation.path())?;
                unfilled_requests.push(download_request);
            }
        } else {
            unfilled_requests.push(download_request);
        }
    }

    if unfilled_requests.is_empty() {
        if matches!(requests.as_slice(), [PythonRequest::Any]) {
            writeln!(
                printer.stderr(),
                "Python is already available. Use `uv python install <request>` to install a specific version.",
            )?;
        } else if requests.len() > 1 {
            writeln!(
                printer.stderr(),
                "All requested versions already installed."
            )?;
        } else {
            writeln!(printer.stderr(), "Requested versions already installed.")?;
        }
        return Ok(ExitStatus::Success);
    }

    let downloads = unfilled_requests
        .into_iter()
        // Populate the download requests with defaults
        .map(PythonDownloadRequest::fill)
        .map(|request| ManagedPythonDownload::from_request(&request))
        .collect::<Result<Vec<_>, uv_python::downloads::Error>>()?;

    // Ensure we only download each version once
    let downloads = downloads
        .into_iter()
        .unique_by(|download| download.key())
        .collect::<Vec<_>>();

    writeln!(
        printer.stderr(),
        "Found {} versions requiring installation",
        downloads.len()
    )?;

    // Construct a client
    let client = uv_client::BaseClientBuilder::new()
        .connectivity(connectivity)
        .native_tls(native_tls)
        .build();

    let mut tasks = futures::stream::iter(downloads.iter())
        .map(|download| async {
            let _ = writeln!(printer.stderr(), "Downloading {}", download.key());
            let result = download.fetch(&client, installations_dir).await;
            (download.python_version(), result)
        })
        .buffered(4);

    let mut results = Vec::new();
    while let Some(task) = tasks.next().await {
        let (version, result) = task;
        let path = match result? {
            // We should only encounter already-available during concurrent installs
            DownloadResult::AlreadyAvailable(path) => path,
            DownloadResult::Fetched(path) => {
                writeln!(
                    printer.stderr(),
                    "Installed Python {version} to {}",
                    path.user_display()
                )?;
                path
            }
        };
        // Ensure the installations have externally managed markers
        let installed = ManagedPythonInstallation::new(path.clone())?;
        installed.ensure_externally_managed()?;
        results.push((version, path));
    }

    let s = if downloads.len() == 1 { "" } else { "s" };
    writeln!(
        printer.stderr(),
        "Installed {} version{s} in {}s",
        downloads.len(),
        start.elapsed().as_secs()
    )?;

    Ok(ExitStatus::Success)
}
