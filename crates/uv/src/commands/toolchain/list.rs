use std::collections::{BTreeSet, HashSet};
use std::fmt::Write;

use anyhow::Result;

use uv_cache::Cache;
use uv_configuration::PreviewMode;
use uv_fs::Simplified;
use uv_toolchain::downloads::PythonDownloadRequest;
use uv_toolchain::{
    find_toolchains, DiscoveryError, EnvironmentPreference, Toolchain, ToolchainNotFound,
    ToolchainPreference, ToolchainRequest, ToolchainSource,
};
use uv_warnings::warn_user_once;

use crate::commands::ExitStatus;
use crate::printer::Printer;
use crate::settings::ToolchainListKinds;

#[derive(Debug, Clone, Eq, PartialEq, PartialOrd, Ord)]
enum Kind {
    Download,
    Managed,
    System,
}

/// List available toolchains.
pub(crate) async fn list(
    kinds: ToolchainListKinds,
    all_versions: bool,
    all_platforms: bool,
    toolchain_preference: ToolchainPreference,
    preview: PreviewMode,
    cache: &Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    if preview.is_disabled() {
        warn_user_once!("`uv toolchain list` is experimental and may change without warning.");
    }

    let download_request = match kinds {
        ToolchainListKinds::Installed => None,
        ToolchainListKinds::Default => Some(if all_platforms {
            PythonDownloadRequest::default()
        } else {
            PythonDownloadRequest::from_env()?
        }),
    };

    let downloads = download_request
        .as_ref()
        .map(uv_toolchain::downloads::PythonDownloadRequest::iter_downloads)
        .into_iter()
        .flatten();

    let installed = find_toolchains(
        &ToolchainRequest::Any,
        EnvironmentPreference::OnlySystem,
        toolchain_preference,
        cache,
    )
    // Raise discovery errors if critical
    .filter(|result| {
        result
            .as_ref()
            .err()
            .map_or(true, DiscoveryError::is_critical)
    })
    .collect::<Result<Vec<Result<Toolchain, ToolchainNotFound>>, DiscoveryError>>()?
    .into_iter()
    // Drop any "missing" toolchains
    .filter_map(std::result::Result::ok);

    let mut output = BTreeSet::new();
    for toolchain in installed {
        let kind = if matches!(toolchain.source(), ToolchainSource::Managed) {
            Kind::Managed
        } else {
            Kind::System
        };
        output.insert((
            toolchain.python_version().clone(),
            toolchain.os().to_string(),
            toolchain.key().clone(),
            kind,
            Some(toolchain.interpreter().sys_executable().to_path_buf()),
        ));
    }
    for download in downloads {
        output.insert((
            download.python_version().version().clone(),
            download.os().to_string(),
            download.key().clone(),
            Kind::Download,
            None,
        ));
    }

    let mut seen_minor = HashSet::new();
    let mut seen_patch = HashSet::new();
    for (version, os, key, kind, path) in output.iter().rev() {
        // Only show the latest patch version for each download unless all were requested
        if !matches!(kind, Kind::System) {
            if let [major, minor, ..] = version.release() {
                if !seen_minor.insert((os.clone(), *major, *minor)) {
                    if matches!(kind, Kind::Download) && !all_versions {
                        continue;
                    }
                }
            }
            if let [major, minor, patch] = version.release() {
                if !seen_patch.insert((os.clone(), *major, *minor, *patch)) {
                    if matches!(kind, Kind::Download) {
                        continue;
                    }
                }
            }
        }
        if let Some(path) = path {
            writeln!(printer.stdout(), "{key}\t{}", path.user_display())?;
        } else {
            writeln!(printer.stdout(), "{key}\t<download available>")?;
        }
    }

    Ok(ExitStatus::Success)
}
