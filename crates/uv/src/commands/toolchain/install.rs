use anyhow::Result;
use std::fmt::Write;
use uv_cache::Cache;
use uv_client::Connectivity;
use uv_configuration::PreviewMode;
use uv_fs::Simplified;
use uv_toolchain::downloads::{DownloadResult, PythonDownload, PythonDownloadRequest};
use uv_toolchain::managed::InstalledToolchains;
use uv_toolchain::ToolchainRequest;
use uv_warnings::warn_user;

use crate::commands::ExitStatus;
use crate::printer::Printer;

/// Download and install a Python toolchain.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn install(
    target: Option<String>,
    native_tls: bool,
    connectivity: Connectivity,
    preview: PreviewMode,
    _cache: &Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    if preview.is_disabled() {
        warn_user!("`uv toolchain fetch` is experimental and may change without warning.");
    }

    let toolchains = InstalledToolchains::from_settings()?.init()?;
    let toolchain_dir = toolchains.root();

    let request = if let Some(target) = target {
        let request = ToolchainRequest::parse(&target);
        match request {
            ToolchainRequest::Any => (),
            ToolchainRequest::Directory(_)
            | ToolchainRequest::ExecutableName(_)
            | ToolchainRequest::File(_) => {
                writeln!(printer.stderr(), "Invalid toolchain request '{target}'")?;
                return Ok(ExitStatus::Failure);
            }
            _ => {
                writeln!(printer.stderr(), "Looking for {request}")?;
            }
        }
        request
    } else {
        writeln!(printer.stderr(), "Using latest Python version")?;
        ToolchainRequest::default()
    };

    if let Some(toolchain) = toolchains
        .find_all()?
        .find(|toolchain| toolchain.satisfies(&request))
    {
        writeln!(
            printer.stderr(),
            "Found installed toolchain '{}'",
            toolchain.key()
        )?;
        writeln!(
            printer.stderr(),
            "Already installed at {}",
            toolchain.path().user_display()
        )?;
        return Ok(ExitStatus::Success);
    }

    // Fill platform information missing from the request
    let request = PythonDownloadRequest::from_request(request)?.fill()?;

    // Find the corresponding download
    let download = PythonDownload::from_request(&request)?;
    let version = download.python_version();

    // Construct a client
    let client = uv_client::BaseClientBuilder::new()
        .connectivity(connectivity)
        .native_tls(native_tls)
        .build();

    writeln!(printer.stderr(), "Downloading {}", download.key())?;
    let result = download.fetch(&client, toolchain_dir).await?;

    let path = match result {
        // Note we should only encounter `AlreadyAvailable` if there's a race condition
        // TODO(zanieb): We should lock the toolchain directory on fetch
        DownloadResult::AlreadyAvailable(path) => path,
        DownloadResult::Fetched(path) => path,
    };

    writeln!(
        printer.stderr(),
        "Installed Python {version} to {}",
        path.user_display()
    )?;

    Ok(ExitStatus::Success)
}
