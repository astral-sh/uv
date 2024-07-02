use anyhow::Result;
use futures::StreamExt;
use itertools::Itertools;
use std::collections::BTreeSet;
use std::fmt::Write;
use uv_configuration::PreviewMode;
use uv_toolchain::downloads::{self, PythonDownloadRequest};
use uv_toolchain::managed::InstalledToolchains;
use uv_toolchain::ToolchainRequest;
use uv_warnings::warn_user_once;

use crate::commands::ExitStatus;
use crate::printer::Printer;

/// Uninstall Python toolchains.
pub(crate) async fn uninstall(
    targets: Vec<String>,
    preview: PreviewMode,
    printer: Printer,
) -> Result<ExitStatus> {
    if preview.is_disabled() {
        warn_user_once!("`uv toolchain uninstall` is experimental and may change without warning.");
    }

    let toolchains = InstalledToolchains::from_settings()?.init()?;
    let _lock = toolchains.acquire_lock()?;

    let requests = targets
        .iter()
        .map(|target| ToolchainRequest::parse(target.as_str()))
        .collect::<Vec<_>>();

    let download_requests = requests
        .iter()
        .map(PythonDownloadRequest::from_request)
        .collect::<Result<Vec<_>, downloads::Error>>()?;

    let installed_toolchains: Vec<_> = toolchains.find_all()?.collect();
    let mut matching_toolchains = BTreeSet::default();
    for (request, download_request) in requests.iter().zip(download_requests) {
        writeln!(
            printer.stderr(),
            "Looking for installed toolchains matching {request} ({download_request})"
        )?;
        let mut found = false;
        for toolchain in installed_toolchains
            .iter()
            .filter(|toolchain| download_request.satisfied_by_key(toolchain.key()))
        {
            found = true;
            if matching_toolchains.insert(toolchain.clone()) {
                writeln!(
                    printer.stderr(),
                    "Found toolchain `{}` that matches {request}",
                    toolchain.key()
                )?;
            }
        }
        if !found {
            writeln!(printer.stderr(), "No toolchains found matching {request}")?;
        }
    }

    if matching_toolchains.is_empty() {
        if matches!(requests.as_slice(), [ToolchainRequest::Any]) {
            writeln!(printer.stderr(), "No installed toolchains found")?;
        } else if requests.len() > 1 {
            writeln!(
                printer.stderr(),
                "No toolchains found matching the requests"
            )?;
        } else {
            writeln!(printer.stderr(), "No toolchains found matching the request")?;
        }
        return Ok(ExitStatus::Failure);
    }

    let tasks = futures::stream::iter(matching_toolchains.iter())
        .map(|toolchain| async {
            (
                toolchain.key(),
                fs_err::tokio::remove_dir_all(toolchain.path()).await,
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
                "Failed to uninstall toolchain `{key}`: {err}"
            )?;
        } else {
            writeln!(printer.stderr(), "Uninstalled toolchain `{key}`")?;
        }
    }

    if failed {
        if matching_toolchains.len() > 1 {
            writeln!(printer.stderr(), "Uninstall of some toolchains failed")?;
        }
        return Ok(ExitStatus::Failure);
    }

    let s = if matching_toolchains.len() == 1 {
        ""
    } else {
        "s"
    };

    writeln!(
        printer.stderr(),
        "Uninstalled {} toolchain{s}",
        matching_toolchains.len()
    )?;

    Ok(ExitStatus::Success)
}
