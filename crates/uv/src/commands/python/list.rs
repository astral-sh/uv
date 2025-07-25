use serde::Serialize;
use std::collections::BTreeSet;
use std::fmt::Write;
use uv_cli::PythonListFormat;
use uv_configuration::Preview;
use uv_pep440::Version;

use anyhow::Result;
use itertools::Either;
use owo_colors::OwoColorize;
use rustc_hash::FxHashSet;
use uv_cache::Cache;
use uv_fs::Simplified;
use uv_python::downloads::PythonDownloadRequest;
use uv_python::{
    DiscoveryError, EnvironmentPreference, PythonDownloads, PythonInstallation, PythonNotFound,
    PythonPreference, PythonRequest, PythonSource, find_python_installations,
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

#[derive(Debug, Serialize)]
struct NamedVersionParts {
    major: u64,
    minor: u64,
    patch: u64,
}

#[derive(Debug, Serialize)]
struct PrintData {
    key: String,
    version: Version,
    version_parts: NamedVersionParts,
    path: Option<String>,
    symlink: Option<String>,
    url: Option<String>,
    os: String,
    variant: String,
    implementation: String,
    arch: String,
    libc: String,
}

/// List available Python installations.
#[allow(clippy::too_many_arguments, clippy::fn_params_excessive_bools)]
pub(crate) async fn list(
    request: Option<String>,
    kinds: PythonListKinds,
    all_versions: bool,
    all_platforms: bool,
    all_arches: bool,
    show_urls: bool,
    output_format: PythonListFormat,
    python_downloads_json_url: Option<String>,
    python_preference: PythonPreference,
    python_downloads: PythonDownloads,
    cache: &Cache,
    printer: Printer,
    preview: Preview,
) -> Result<ExitStatus> {
    let request = request.as_deref().map(PythonRequest::parse);
    let base_download_request = if python_preference == PythonPreference::OnlySystem {
        None
    } else {
        // If the user request cannot be mapped to a download request, we won't show any downloads
        PythonDownloadRequest::from_request(request.as_ref().unwrap_or(&PythonRequest::Any))
    };

    let mut output = BTreeSet::new();
    if let Some(base_download_request) = base_download_request {
        let download_request = match kinds {
            PythonListKinds::Installed => None,
            PythonListKinds::Downloads => Some(if all_platforms {
                base_download_request
            } else if all_arches {
                base_download_request.fill_platform()?.with_any_arch()
            } else {
                base_download_request.fill_platform()?
            }),
            PythonListKinds::Default => {
                if python_downloads.is_automatic() {
                    Some(if all_platforms {
                        base_download_request
                    } else if all_arches {
                        base_download_request.fill_platform()?.with_any_arch()
                    } else {
                        base_download_request.fill_platform()?
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
            .map(|a| PythonDownloadRequest::iter_downloads(a, python_downloads_json_url.as_deref()))
            .transpose()?
            .into_iter()
            .flatten();

        for download in downloads {
            output.insert((
                download.key().clone(),
                Kind::Download,
                Either::Right(download.url()),
            ));
        }
    }

    let installed =
        match kinds {
            PythonListKinds::Installed | PythonListKinds::Default => {
                Some(find_python_installations(
                request.as_ref().unwrap_or(&PythonRequest::Any),
                EnvironmentPreference::OnlySystem,
                python_preference,
                cache,
                preview,
            )
            // Raise discovery errors if critical
            .filter(|result| {
                result
                    .as_ref()
                    .err()
                    .is_none_or(DiscoveryError::is_critical)
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
                Either::Left(installation.interpreter().real_executable().to_path_buf()),
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

        // Only show the latest patch version for each download unless all were requested.
        //
        // We toggle off platforms/arches based unless all_platforms/all_arches because
        // we want to only show the "best" option for each version by default, even
        // if e.g. the x86_32 build would also work on x86_64.
        if !matches!(kind, Kind::System) {
            if let [major, minor, ..] = *key.version().release() {
                if !seen_minor.insert((
                    all_platforms.then_some(*key.os()),
                    major,
                    minor,
                    key.variant(),
                    key.implementation(),
                    all_arches.then_some(*key.arch()),
                    *key.libc(),
                )) {
                    if matches!(kind, Kind::Download) && !all_versions {
                        continue;
                    }
                }
            }
            if let [major, minor, patch] = *key.version().release() {
                if !seen_patch.insert((
                    all_platforms.then_some(*key.os()),
                    major,
                    minor,
                    patch,
                    key.variant(),
                    key.implementation(),
                    all_arches.then_some(*key.arch()),
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

    match output_format {
        PythonListFormat::Json => {
            let data = include
                .iter()
                .map(|(key, uri)| -> Result<_> {
                    let mut path_or_none: Option<String> = None;
                    let mut symlink_or_none: Option<String> = None;
                    let mut url_or_none: Option<String> = None;
                    match uri {
                        Either::Left(path) => {
                            path_or_none = Some(path.user_display().to_string());

                            let is_symlink = fs_err::symlink_metadata(path)?.is_symlink();
                            if is_symlink {
                                symlink_or_none =
                                    Some(path.read_link()?.user_display().to_string());
                            }
                        }
                        Either::Right(url) => {
                            url_or_none = Some((*url).to_string());
                        }
                    }
                    let version = key.version();
                    let release = version.release();

                    Ok(PrintData {
                        key: key.to_string(),
                        version: version.version().clone(),
                        #[allow(clippy::get_first)]
                        version_parts: NamedVersionParts {
                            major: release.get(0).copied().unwrap_or(0),
                            minor: release.get(1).copied().unwrap_or(0),
                            patch: release.get(2).copied().unwrap_or(0),
                        },
                        path: path_or_none,
                        symlink: symlink_or_none,
                        url: url_or_none,
                        arch: key.arch().to_string(),
                        implementation: key.implementation().to_string(),
                        os: key.os().to_string(),
                        variant: key.variant().to_string(),
                        libc: key.libc().to_string(),
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            writeln!(printer.stdout(), "{}", serde_json::to_string(&data)?)?;
        }
        PythonListFormat::Text => {
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
        }
    }

    Ok(ExitStatus::Success)
}
