use std::fmt::Write;
use std::ops::Deref;

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

    let downloads = match includes {
        ToolchainListIncludes::All => {
            let request = PythonDownloadRequest::default();
            request.iter_downloads().collect()
        }
        ToolchainListIncludes::Installed => Vec::new(),
        ToolchainListIncludes::Default => {
            let request = PythonDownloadRequest::from_env()?;
            request.iter_downloads().collect()
        }
    };

    let installed = {
        InstalledToolchains::from_settings()?
            .init()?
            .find_all()?
            .collect_vec()
    };

    let mut output = Vec::new();
    for toolchain in installed {
        output.push((
            toolchain.python_version().deref().version.clone(),
            toolchain.key().to_owned(),
        ));
    }
    for download in downloads {
        output.push((
            download.python_version().deref().version.clone(),
            download.key().to_owned(),
        ));
    }

    output.sort();
    output.dedup();

    for (version, key) in output {
        writeln!(printer.stdout(), "{:<8} ({key})", version.to_string())?;
    }

    Ok(ExitStatus::Success)
}
