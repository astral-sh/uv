use std::collections::BTreeSet;
use std::fmt::Write;

use anyhow::Result;
use itertools::Either;
use owo_colors::OwoColorize;
use rustc_hash::FxHashSet;
use uv_cache::Cache;
use uv_fs::Simplified;
use uv_python::downloads::PythonDownloadRequest;
use uv_python::{
    find_python_installations, DiscoveryError, EnvironmentPreference, PythonDownloads,
    PythonInstallation, PythonNotFound, PythonPreference, PythonRequest, PythonSource,
};

use crate::commands::ExitStatus;
use crate::printer::Printer;
use crate::settings::PythonListKinds;

#[derive(Debug, Clone, Eq, PartialEq, PartialOrd, Ord)]
enum Kind {
    Download,
    Managed,
    System,
}

/// List available Python installations.
#[allow(clippy::too_many_arguments, clippy::fn_params_excessive_bools)]
pub(crate) async fn list(
    kinds: PythonListKinds,
    all_versions: bool,
    all_platforms: bool,
    all_arches: bool,
    show_urls: bool,
    python_preference: PythonPreference,
    python_downloads: PythonDownloads,
    cache: &Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    let mut output = BTreeSet::new();
    if python_preference != PythonPreference::OnlySystem {
        let download_request = match kinds {
            PythonListKinds::Installed => None,
            PythonListKinds::Downloads => Some(if all_platforms {
                PythonDownloadRequest::default()
            } else {
                PythonDownloadRequest::from_env()?
            }),
            PythonListKinds::Default => {
                if python_downloads.is_automatic() {
                    Some(if all_platforms {
                        PythonDownloadRequest::default()
                    } else if all_arches {
                        PythonDownloadRequest::from_env()?.with_any_arch()
                    } else {
                        PythonDownloadRequest::from_env()?
                    })
                } else {
                    // If fetching is not automatic, then don't show downloads as available by default
                    None
                }
            }
        }
        // Include pre-release versions
        .map(|request| request.with_prereleases(true));

        let downloads = download_request
            .as_ref()
            .map(PythonDownloadRequest::iter_downloads)
            .into_iter()
            .flatten();

        for download in downloads {
            output.insert((
                download.key().clone(),
                Kind::Download,
                Either::Right(download.url()),
            ));
        }
    };

    let installed =
        match kinds {
            PythonListKinds::Installed | PythonListKinds::Default => {
                Some(find_python_installations(
                &PythonRequest::Any,
                EnvironmentPreference::OnlySystem,
                python_preference,
                cache,
            )
            // Raise discovery errors if critical
            .filter(|result| {
                result
                    .as_ref()
                    .err()
                    .map_or(true, DiscoveryError::is_critical)
            })
            .collect::<Result<Vec<Result<PythonInstallation, PythonNotFound>>, DiscoveryError>>()?
            .into_iter()
            // Drop any "missing" installations
            .filter_map(Result::ok))
            }
            PythonListKinds::Downloads => None,
        };

    if let Some(installed) = installed {
        for installation in installed {
            let kind = if matches!(installation.source(), PythonSource::Managed) {
                Kind::Managed
            } else {
                Kind::System
            };
            output.insert((
                installation.key(),
                kind,
                Either::Left(installation.interpreter().sys_executable().to_path_buf()),
            ));
        }
    }

    let mut seen_minor = FxHashSet::default();
    let mut seen_patch = FxHashSet::default();
    let mut seen_paths = FxHashSet::default();
    let mut include = Vec::new();
    for (key, kind, uri) in output.iter().rev() {
        // Do not show the same path more than once
        if let Either::Left(path) = uri {
            if !seen_paths.insert(path) {
                continue;
            }
        }

        // Only show the latest patch version for each download unless all were requested
        if !matches!(kind, Kind::System) {
            if let [major, minor, ..] = *key.version().release() {
                if !seen_minor.insert((
                    *key.os(),
                    major,
                    minor,
                    key.variant(),
                    key.implementation(),
                    *key.arch(),
                    *key.libc(),
                )) {
                    if matches!(kind, Kind::Download) && !all_versions {
                        continue;
                    }
                }
            }
            if let [major, minor, patch] = *key.version().release() {
                if !seen_patch.insert((
                    *key.os(),
                    major,
                    minor,
                    patch,
                    key.variant(),
                    key.implementation(),
                    *key.arch(),
                    key.libc(),
                )) {
                    if matches!(kind, Kind::Download) {
                        continue;
                    }
                }
            }
        }
        include.push((key, uri));
    }

    // Compute the width of the first column.
    let width = include
        .iter()
        .fold(0usize, |acc, (key, _)| acc.max(key.to_string().len()));

    for (key, uri) in include {
        let key = key.to_string();
        match uri {
            Either::Left(path) => {
                let is_symlink = fs_err::symlink_metadata(path)?.is_symlink();
                if is_symlink {
                    writeln!(
                        printer.stdout(),
                        "{key:width$}    {} -> {}",
                        path.user_display().cyan(),
                        path.read_link()?.user_display().cyan()
                    )?;
                } else {
                    writeln!(
                        printer.stdout(),
                        "{key:width$}    {}",
                        path.user_display().cyan()
                    )?;
                }
            }
            Either::Right(url) => {
                if show_urls {
                    writeln!(printer.stdout(), "{key:width$}    {}", url.dimmed())?;
                } else {
                    writeln!(
                        printer.stdout(),
                        "{key:width$}    {}",
                        "<download available>".dimmed()
                    )?;
                }
            }
        }
    }

    Ok(ExitStatus::Success)
}
