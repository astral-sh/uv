use anyhow::Result;
use futures::StreamExt;
use itertools::Itertools;
use std::fmt::Write;
use uv_cache::Cache;
use uv_client::Connectivity;
use uv_configuration::PreviewMode;
use uv_fs::Simplified;
use uv_toolchain::downloads::{self, DownloadResult, PythonDownload, PythonDownloadRequest};
use uv_toolchain::managed::{InstalledToolchain, InstalledToolchains};
use uv_toolchain::{requests_from_version_file, ToolchainRequest};
use uv_warnings::warn_user_once;

use crate::commands::ExitStatus;
use crate::printer::Printer;

/// Download and install a Python toolchain.
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
        warn_user_once!("`uv toolchain install` is experimental and may change without warning.");
    }

    let start = std::time::Instant::now();

    let toolchains = InstalledToolchains::from_settings()?.init()?;
    let toolchain_dir = toolchains.root();

    let requests: Vec<_> = if targets.is_empty() {
        if let Some(requests) = requests_from_version_file().await? {
            requests
        } else {
            vec![ToolchainRequest::Any]
        }
    } else {
        targets
            .iter()
            .map(|target| ToolchainRequest::parse(target.as_str()))
            .collect()
    };

    let download_requests = requests
        .iter()
        .map(|request| PythonDownloadRequest::from_request(request.clone()))
        .collect::<Result<Vec<_>, downloads::Error>>()?;

    let installed_toolchains: Vec<_> = toolchains.find_all()?.collect();
    let mut unfilled_requests = Vec::new();
    for (request, download_request) in requests.iter().zip(download_requests) {
        writeln!(
            printer.stderr(),
            "Looking for toolchain {request} ({download_request})"
        )?;
        if let Some(toolchain) = installed_toolchains
            .iter()
            .find(|toolchain| download_request.satisfied_by_key(toolchain.key()))
        {
            writeln!(
                printer.stderr(),
                "Found installed toolchain '{}' that satisfies {request}",
                toolchain.key()
            )?;
            if force {
                unfilled_requests.push(download_request);
            }
        } else {
            unfilled_requests.push(download_request);
        }
    }

    if unfilled_requests.is_empty() {
        if matches!(requests.as_slice(), [ToolchainRequest::Any]) {
            writeln!(
                printer.stderr(),
                "A toolchain is already installed. Use `uv toolchain install <request>` to install a specific toolchain.",
            )?;
        } else if requests.len() > 1 {
            writeln!(
                printer.stderr(),
                "All requested toolchains already installed."
            )?;
        } else {
            writeln!(printer.stderr(), "Requested toolchain already installed.")?;
        }
        return Ok(ExitStatus::Success);
    }

    if unfilled_requests.len() > 1 {
        writeln!(
            printer.stderr(),
            "Found {}/{} toolchains requiring installation",
            unfilled_requests.len(),
            requests.len()
        )?;
    }

    let downloads = unfilled_requests
        .into_iter()
        // Populate the download requests with defaults
        .map(PythonDownloadRequest::fill)
        .map(|request| request.map(|inner| PythonDownload::from_request(&inner)))
        .flatten_ok()
        .collect::<Result<Vec<_>, uv_toolchain::downloads::Error>>()?;

    // Construct a client
    let client = uv_client::BaseClientBuilder::new()
        .connectivity(connectivity)
        .native_tls(native_tls)
        .build();

    let mut tasks = futures::stream::iter(downloads.iter())
        .map(|download| async {
            let _ = writeln!(printer.stderr(), "Downloading {}", download.key());
            let result = download.fetch(&client, toolchain_dir).await;
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
        let installed = InstalledToolchain::new(path.clone())?;
        installed.ensure_externally_managed()?;
        results.push((version, path));
    }

    let s = if downloads.len() == 1 { "" } else { "s" };
    writeln!(
        printer.stderr(),
        "Installed {} toolchain{s} in {}s",
        downloads.len(),
        start.elapsed().as_secs()
    )?;

    Ok(ExitStatus::Success)
}
