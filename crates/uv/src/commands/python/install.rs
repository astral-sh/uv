use std::collections::BTreeSet;
use std::fmt::Write;
use std::path::PathBuf;

use anyhow::Result;
use fs_err as fs;
use futures::StreamExt;
use itertools::Itertools;
use owo_colors::OwoColorize;
use tracing::debug;
use uv_cache::Cache;
use uv_client::Connectivity;
use uv_configuration::PreviewMode;
use uv_fs::Simplified;
use uv_python::downloads::{self, DownloadResult, ManagedPythonDownload, PythonDownloadRequest};
use uv_python::managed::{ManagedPythonInstallation, ManagedPythonInstallations};
use uv_python::{
    requests_from_version_file, PythonRequest, PYTHON_VERSIONS_FILENAME, PYTHON_VERSION_FILENAME,
};
use uv_warnings::warn_user_once;

use crate::commands::reporters::PythonDownloadReporter;
use crate::commands::{elapsed, ExitStatus};
use crate::printer::Printer;

/// Download and install Python versions.
pub(crate) async fn install(
    targets: Vec<String>,
    reinstall: bool,
    native_tls: bool,
    connectivity: Connectivity,
    preview: PreviewMode,
    isolated: bool,
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
        // Read from the version file, unless `isolated` was requested
        let version_file_requests = if isolated {
            if PathBuf::from(PYTHON_VERSION_FILENAME).exists() {
                debug!("Ignoring `.python-version` file due to isolated mode");
            } else if PathBuf::from(PYTHON_VERSIONS_FILENAME).exists() {
                debug!("Ignoring `.python-versions` file due to isolated mode");
            }
            None
        } else {
            requests_from_version_file().await?
        };
        version_file_requests.unwrap_or_else(|| vec![PythonRequest::Any])
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
        if matches!(requests.as_slice(), [PythonRequest::Any]) {
            writeln!(printer.stderr(), "Searching for Python installations")?;
        } else {
            writeln!(
                printer.stderr(),
                "Searching for Python versions matching: {}",
                request.cyan()
            )?;
        }
        if let Some(installation) = installed_installations
            .iter()
            .find(|installation| download_request.satisfied_by_key(installation.key()))
        {
            if matches!(request, PythonRequest::Any) {
                writeln!(printer.stderr(), "Found: {}", installation.key().green(),)?;
            } else {
                writeln!(
                    printer.stderr(),
                    "Found existing installation for {}: {}",
                    request.cyan(),
                    installation.key().green(),
                )?;
            }
            if reinstall {
                writeln!(
                    printer.stderr(),
                    "Uninstalling {}",
                    installation.key().green()
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
            writeln!(printer.stderr(), "All requested versions already installed")?;
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

    // Construct a client
    let client = uv_client::BaseClientBuilder::new()
        .connectivity(connectivity)
        .native_tls(native_tls)
        .build();

    let reporter = PythonDownloadReporter::new(printer, downloads.len() as u64);

    let results = futures::stream::iter(downloads.iter())
        .map(|download| async {
            let result = download
                .fetch(&client, installations_dir, Some(&reporter))
                .await;
            (download.python_version(), result)
        })
        .buffered(4)
        .collect::<Vec<_>>()
        .await;

    for (version, result) in results {
        let path = match result? {
            // We should only encounter already-available during concurrent installs
            DownloadResult::AlreadyAvailable(path) => path,
            DownloadResult::Fetched(path) => {
                writeln!(
                    printer.stderr(),
                    "Installed {} to: {}",
                    format!("Python {version}").cyan(),
                    path.user_display().cyan()
                )?;
                path
            }
        };

        // Ensure the installations have externally managed markers
        let installed = ManagedPythonInstallation::new(path.clone())?;
        installed.ensure_externally_managed()?;
    }

    let s = if downloads.len() == 1 { "" } else { "s" };
    writeln!(
        printer.stderr(),
        "{}",
        format!(
            "Installed {} {}",
            format!("{} version{s}", downloads.len()).bold(),
            format!("in {}", elapsed(start.elapsed())).dimmed()
        )
        .dimmed()
    )?;

    Ok(ExitStatus::Success)
}
