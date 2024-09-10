use std::collections::BTreeSet;
use std::fmt::Write;

use anyhow::Result;
use fs_err as fs;
use futures::stream::FuturesUnordered;
use futures::StreamExt;
use itertools::Itertools;
use owo_colors::OwoColorize;

use uv_client::Connectivity;
use uv_fs::CWD;
use uv_python::downloads::{DownloadResult, ManagedPythonDownload, PythonDownloadRequest};
use uv_python::managed::{ManagedPythonInstallation, ManagedPythonInstallations};
use uv_python::{PythonDownloads, PythonRequest, PythonVersionFile};

use crate::commands::python::{ChangeEvent, ChangeEventKind};
use crate::commands::reporters::PythonDownloadReporter;
use crate::commands::{elapsed, ExitStatus};
use crate::printer::Printer;

/// Download and install Python versions.
pub(crate) async fn install(
    targets: Vec<String>,
    reinstall: bool,
    python_downloads: PythonDownloads,
    native_tls: bool,
    connectivity: Connectivity,
    no_config: bool,
    printer: Printer,
) -> Result<ExitStatus> {
    let start = std::time::Instant::now();

    let installations = ManagedPythonInstallations::from_settings()?.init()?;
    let installations_dir = installations.root();
    let cache_dir = installations.cache();
    let _lock = installations.lock().await?;

    let targets = targets.into_iter().collect::<BTreeSet<_>>();
    let requests: Vec<_> = if targets.is_empty() {
        PythonVersionFile::discover(&*CWD, no_config, true)
            .await?
            .map(uv_python::PythonVersionFile::into_versions)
            .unwrap_or_else(|| vec![PythonRequest::Any])
    } else {
        targets
            .iter()
            .map(|target| PythonRequest::parse(target.as_str()))
            .collect()
    };

    let download_requests = requests
        .iter()
        .map(|request| {
            PythonDownloadRequest::from_request(request).ok_or_else(|| {
                anyhow::anyhow!("Cannot download managed Python for request: {request}")
            })
        })
        .collect::<Result<Vec<_>>>()?;

    let installed_installations: Vec<_> = installations.find_all()?.collect();
    let mut unfilled_requests = Vec::new();
    let mut uninstalled = Vec::new();
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
                writeln!(printer.stderr(), "Found: {}", installation.key().green())?;
            } else {
                writeln!(
                    printer.stderr(),
                    "Found existing installation for {}: {}",
                    request.cyan(),
                    installation.key().green(),
                )?;
            }
            if reinstall {
                fs::remove_dir_all(installation.path())?;
                uninstalled.push(installation.key().clone());
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

    if matches!(python_downloads, PythonDownloads::Never) {
        writeln!(
            printer.stderr(),
            "Python downloads are not allowed (`python-downloads = \"never\"`). Change to `python-downloads = \"manual\"` to allow explicit installs.",
        )?;
        return Ok(ExitStatus::Failure);
    }

    let downloads = unfilled_requests
        .into_iter()
        // Populate the download requests with defaults
        .map(|request| ManagedPythonDownload::from_request(&PythonDownloadRequest::fill(request)?))
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

    let mut tasks = FuturesUnordered::new();
    for download in &downloads {
        tasks.push(async {
            (
                download.key(),
                download
                    .fetch(&client, installations_dir, &cache_dir, Some(&reporter))
                    .await,
            )
        });
    }

    let mut installed = vec![];
    let mut errors = vec![];
    while let Some((key, result)) = tasks.next().await {
        match result {
            Ok(download) => {
                let path = match download {
                    // We should only encounter already-available during concurrent installs
                    DownloadResult::AlreadyAvailable(path) => path,
                    DownloadResult::Fetched(path) => path,
                };

                installed.push(key.clone());

                // Ensure the installations have externally managed markers
                let managed = ManagedPythonInstallation::new(path.clone())?;
                managed.ensure_externally_managed()?;
            }
            Err(err) => {
                errors.push((key, err));
            }
        }
    }

    if !installed.is_empty() {
        if let [installed] = installed.as_slice() {
            // Ex) "Installed Python 3.9.7 in 1.68s"
            writeln!(
                printer.stderr(),
                "{}",
                format!(
                    "Installed {} {}",
                    format!("Python {}", installed.version()).bold(),
                    format!("in {}", elapsed(start.elapsed())).dimmed()
                )
                .dimmed()
            )?;
        } else {
            // Ex) "Installed 2 versions in 1.68s"
            let s = if installed.len() == 1 { "" } else { "s" };
            writeln!(
                printer.stderr(),
                "{}",
                format!(
                    "Installed {} {}",
                    format!("{} version{s}", installed.len()).bold(),
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
            .chain(installed.into_iter().map(|key| ChangeEvent {
                key,
                kind: ChangeEventKind::Added,
            }))
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
                "{}: Failed to install {}",
                "error".red().bold(),
                key.green()
            )?;
            for err in anyhow::Error::new(err).chain() {
                writeln!(printer.stderr(), "  {}: {}", "Caused by".red().bold(), err)?;
            }
        }
        return Ok(ExitStatus::Failure);
    }

    Ok(ExitStatus::Success)
}
