use std::collections::BTreeSet;
use std::fmt::Write;

use anyhow::Result;
use itertools::Itertools;

use uv_cache::Cache;
use uv_configuration::PreviewMode;
use uv_toolchain::downloads::PythonDownloadRequest;
use uv_toolchain::managed::InstalledToolchains;
use uv_warnings::warn_user;

use crate::commands::ExitStatus;
use crate::printer::Printer;
use crate::settings::ToolchainListIncludes;

/// List available toolchains.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn list(
    includes: ToolchainListIncludes,
    preview: PreviewMode,
    _cache: &Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    if preview.is_disabled() {
        warn_user!("`uv toolchain list` is experimental and may change without warning.");
    }

    let download_request = match includes {
        ToolchainListIncludes::All => Some(PythonDownloadRequest::default()),
        ToolchainListIncludes::Installed => None,
        ToolchainListIncludes::Default => Some(PythonDownloadRequest::from_env()?),
    };

    let downloads = download_request
        .as_ref()
        .map(uv_toolchain::downloads::PythonDownloadRequest::iter_downloads)
        .into_iter()
        .flatten();

    let installed = {
        InstalledToolchains::from_settings()?
            .init()?
            .find_all()?
            .collect_vec()
    };

    // Sort and de-duplicate the output.
    let mut output = BTreeSet::new();
    for toolchain in installed {
        output.insert((
            toolchain.python_version().version().clone(),
            toolchain.key().to_owned(),
        ));
    }
    for download in downloads {
        output.insert((
            download.python_version().version().clone(),
            download.key().to_owned(),
        ));
    }

    for (version, key) in output {
        writeln!(printer.stdout(), "{:<8} ({key})", version.to_string())?;
    }

    Ok(ExitStatus::Success)
}
