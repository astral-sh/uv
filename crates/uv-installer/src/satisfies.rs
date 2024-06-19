use std::fmt::Debug;
use std::path::Path;

use anyhow::Result;
use same_file::is_same_file;
use serde::Deserialize;
use tracing::{debug, trace};
use url::Url;

use cache_key::{CanonicalUrl, RepositoryUrl};
use distribution_types::{InstalledDirectUrlDist, InstalledDist};
use pypi_types::{DirInfo, DirectUrl, RequirementSource, VcsInfo, VcsKind};
use uv_cache::{ArchiveTarget, ArchiveTimestamp};

#[derive(Debug, Copy, Clone)]
pub(crate) enum RequirementSatisfaction {
    Mismatch,
    Satisfied,
    OutOfDate,
    Dynamic,
}

impl RequirementSatisfaction {
    /// Returns true if a requirement is satisfied by an installed distribution.
    ///
    /// Returns an error if IO fails during a freshness check for a local path.
    pub(crate) fn check(distribution: &InstalledDist, source: &RequirementSource) -> Result<Self> {
        trace!(
            "Comparing installed with source: {:?} {:?}",
            distribution,
            source
        );
        // Filter out already-installed packages.
        match source {
            // If the requirement comes from a registry, check by name.
            RequirementSource::Registry { specifier, .. } => {
                if specifier.contains(distribution.version()) {
                    return Ok(Self::Satisfied);
                }
                Ok(Self::Mismatch)
            }
            RequirementSource::Url {
                // We use the location since `direct_url.json` also stores this URL, e.g.
                // `pip install git+https://github.com/tqdm/tqdm@cc372d09dcd5a5eabdc6ed4cf365bdb0be004d44#subdirectory=.`
                // records `"url": "https://github.com/tqdm/tqdm"` in `direct_url.json`.
                location: requested_url,
                subdirectory: requested_subdirectory,
                url: _,
            } => {
                let InstalledDist::Url(InstalledDirectUrlDist {
                    direct_url,
                    editable,
                    ..
                }) = &distribution
                else {
                    return Ok(Self::Mismatch);
                };
                let DirectUrl::ArchiveUrl {
                    url: installed_url,
                    archive_info: _,
                    subdirectory: installed_subdirectory,
                } = direct_url.as_ref()
                else {
                    return Ok(Self::Mismatch);
                };

                if *editable {
                    return Ok(Self::Mismatch);
                }

                if requested_subdirectory != installed_subdirectory {
                    return Ok(Self::Mismatch);
                }

                if !CanonicalUrl::parse(installed_url)
                    .is_ok_and(|installed_url| installed_url == CanonicalUrl::new(requested_url))
                {
                    return Ok(Self::Mismatch);
                }

                // If the requirement came from a local path, check freshness.
                if requested_url.scheme() == "file" {
                    if let Ok(archive) = requested_url.to_file_path() {
                        if !ArchiveTimestamp::up_to_date_with(
                            &archive,
                            ArchiveTarget::Install(distribution),
                        )? {
                            return Ok(Self::OutOfDate);
                        }
                    }
                }

                // Otherwise, assume the requirement is up-to-date.
                Ok(Self::Satisfied)
            }
            RequirementSource::Git {
                url: _,
                repository: requested_repository,
                reference: requested_reference,
                precise: requested_precise,
                subdirectory: requested_subdirectory,
            } => {
                let InstalledDist::Url(InstalledDirectUrlDist { direct_url, .. }) = &distribution
                else {
                    return Ok(Self::Mismatch);
                };
                let DirectUrl::VcsUrl {
                    url: installed_url,
                    vcs_info:
                        VcsInfo {
                            vcs: VcsKind::Git,
                            requested_revision: installed_reference,
                            commit_id: _,
                        },
                    subdirectory: installed_subdirectory,
                } = direct_url.as_ref()
                else {
                    return Ok(Self::Mismatch);
                };

                if requested_subdirectory != installed_subdirectory {
                    debug!(
                        "Subdirectory mismatch: {:?} vs. {:?}",
                        installed_subdirectory, requested_subdirectory
                    );
                    return Ok(Self::Mismatch);
                }

                if !RepositoryUrl::parse(installed_url).is_ok_and(|installed_url| {
                    installed_url == RepositoryUrl::new(requested_repository)
                }) {
                    debug!(
                        "Repository mismatch: {:?} vs. {:?}",
                        installed_url, requested_repository
                    );
                    return Ok(Self::Mismatch);
                }

                if installed_reference.as_deref() != requested_reference.as_str()
                    && installed_reference != &requested_precise.map(|git_sha| git_sha.to_string())
                {
                    debug!(
                        "Reference mismatch: {:?} vs. {:?} and {:?}",
                        installed_reference, requested_reference, requested_precise
                    );
                    return Ok(Self::OutOfDate);
                }

                Ok(Self::Satisfied)
            }
            RequirementSource::Path {
                install_path: requested_path,
                lock_path: _,
                url: _,
            } => {
                let InstalledDist::Url(InstalledDirectUrlDist { direct_url, .. }) = &distribution
                else {
                    return Ok(Self::Mismatch);
                };
                let DirectUrl::ArchiveUrl {
                    url: installed_url,
                    archive_info: _,
                    subdirectory: None,
                } = direct_url.as_ref()
                else {
                    return Ok(Self::Mismatch);
                };

                let Some(installed_path) = Url::parse(installed_url)
                    .ok()
                    .and_then(|url| url.to_file_path().ok())
                else {
                    return Ok(Self::Mismatch);
                };

                if !(*requested_path == installed_path
                    || is_same_file(requested_path, &installed_path).unwrap_or(false))
                {
                    trace!(
                        "Path mismatch: {:?} vs. {:?}",
                        requested_path,
                        installed_path,
                    );
                    return Ok(Self::Mismatch);
                }

                if !ArchiveTimestamp::up_to_date_with(
                    requested_path,
                    ArchiveTarget::Install(distribution),
                )? {
                    trace!("Installed package is out of date");
                    return Ok(Self::OutOfDate);
                }

                Ok(Self::Satisfied)
            }
            RequirementSource::Directory {
                install_path: requested_path,
                lock_path: _,
                editable: requested_editable,
                url: _,
            } => {
                let InstalledDist::Url(InstalledDirectUrlDist { direct_url, .. }) = &distribution
                else {
                    return Ok(Self::Mismatch);
                };
                let DirectUrl::LocalDirectory {
                    url: installed_url,
                    dir_info:
                        DirInfo {
                            editable: installed_editable,
                        },
                } = direct_url.as_ref()
                else {
                    return Ok(Self::Mismatch);
                };

                if *requested_editable != installed_editable.unwrap_or_default() {
                    trace!(
                        "Editable mismatch: {:?} vs. {:?}",
                        *requested_editable,
                        installed_editable.unwrap_or_default()
                    );
                    return Ok(Self::Mismatch);
                }

                let Some(installed_path) = Url::parse(installed_url)
                    .ok()
                    .and_then(|url| url.to_file_path().ok())
                else {
                    return Ok(Self::Mismatch);
                };

                if !(*requested_path == installed_path
                    || is_same_file(requested_path, &installed_path).unwrap_or(false))
                {
                    trace!(
                        "Path mismatch: {:?} vs. {:?}",
                        requested_path,
                        installed_path,
                    );
                    return Ok(Self::Mismatch);
                }

                if !ArchiveTimestamp::up_to_date_with(
                    requested_path,
                    ArchiveTarget::Install(distribution),
                )? {
                    trace!("Installed package is out of date");
                    return Ok(Self::OutOfDate);
                }

                // Does the package have dynamic metadata?
                if is_dynamic(requested_path) {
                    trace!("Dependency is dynamic");
                    return Ok(Self::Dynamic);
                }

                Ok(Self::Satisfied)
            }
        }
    }
}

/// Returns `true` if the source tree at the given path contains dynamic metadata.
fn is_dynamic(path: &Path) -> bool {
    // If there's no `pyproject.toml`, we assume it's dynamic.
    let Ok(contents) = fs_err::read_to_string(path.join("pyproject.toml")) else {
        return true;
    };
    let Ok(pyproject_toml) = toml::from_str::<PyProjectToml>(&contents) else {
        return true;
    };
    // If `[project]` is not present, we assume it's dynamic.
    let Some(project) = pyproject_toml.project else {
        // ...unless it appears to be a Poetry project.
        return pyproject_toml
            .tool
            .map_or(true, |tool| tool.poetry.is_none());
    };
    // `[project.dynamic]` must be present and non-empty.
    project.dynamic.is_some_and(|dynamic| !dynamic.is_empty())
}

/// A pyproject.toml as specified in PEP 517.
#[derive(Deserialize, Debug)]
#[serde(rename_all = "kebab-case")]
struct PyProjectToml {
    project: Option<Project>,
    tool: Option<Tool>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "kebab-case")]
struct Project {
    dynamic: Option<Vec<String>>,
}

#[derive(Deserialize, Debug)]
struct Tool {
    poetry: Option<ToolPoetry>,
}

#[derive(Deserialize, Debug)]
struct ToolPoetry {
    #[allow(dead_code)]
    name: Option<String>,
}
