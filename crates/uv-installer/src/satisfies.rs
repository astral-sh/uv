use std::fmt::Debug;

use same_file::is_same_file;
use tracing::{debug, trace};
use url::Url;

use uv_cache_info::CacheInfo;
use uv_cache_key::{CanonicalUrl, RepositoryUrl};
use uv_distribution_types::{InstalledDirectUrlDist, InstalledDist, RequirementSource};
use uv_git_types::GitOid;
use uv_pypi_types::{DirInfo, DirectUrl, VcsInfo, VcsKind};

#[derive(Debug, Copy, Clone)]
pub(crate) enum RequirementSatisfaction {
    Mismatch,
    Satisfied,
    OutOfDate,
    CacheInvalid,
}

impl RequirementSatisfaction {
    /// Returns true if a requirement is satisfied by an installed distribution.
    ///
    /// Returns an error if IO fails during a freshness check for a local path.
    pub(crate) fn check(distribution: &InstalledDist, source: &RequirementSource) -> Self {
        trace!(
            "Comparing installed with source: {:?} {:?}",
            distribution, source
        );
        // Filter out already-installed packages.
        match source {
            // If the requirement comes from a registry, check by name.
            RequirementSource::Registry { specifier, .. } => {
                if specifier.contains(distribution.version()) {
                    return Self::Satisfied;
                }
                Self::Mismatch
            }
            RequirementSource::Url {
                // We use the location since `direct_url.json` also stores this URL, e.g.
                // `pip install git+https://github.com/tqdm/tqdm@cc372d09dcd5a5eabdc6ed4cf365bdb0be004d44#subdirectory=.`
                // records `"url": "https://github.com/tqdm/tqdm"` in `direct_url.json`.
                location: requested_url,
                subdirectory: requested_subdirectory,
                ext: _,
                url: _,
            } => {
                let InstalledDist::Url(InstalledDirectUrlDist {
                    direct_url,
                    editable,
                    cache_info,
                    ..
                }) = &distribution
                else {
                    return Self::Mismatch;
                };
                let DirectUrl::ArchiveUrl {
                    url: installed_url,
                    archive_info: _,
                    subdirectory: installed_subdirectory,
                } = direct_url.as_ref()
                else {
                    return Self::Mismatch;
                };

                if *editable {
                    return Self::Mismatch;
                }

                if requested_subdirectory != installed_subdirectory {
                    return Self::Mismatch;
                }

                if !CanonicalUrl::parse(installed_url)
                    .is_ok_and(|installed_url| installed_url == CanonicalUrl::new(requested_url))
                {
                    return Self::Mismatch;
                }

                // If the requirement came from a local path, check freshness.
                if requested_url.scheme() == "file" {
                    if let Ok(archive) = requested_url.to_file_path() {
                        let Some(cache_info) = cache_info.as_ref() else {
                            return Self::OutOfDate;
                        };
                        match CacheInfo::from_path(&archive) {
                            Ok(read_cache_info) => {
                                if *cache_info != read_cache_info {
                                    return Self::OutOfDate;
                                }
                            }
                            Err(err) => {
                                debug!(
                                    "Failed to read cached requirement for: {distribution} ({err})"
                                );
                                return Self::CacheInvalid;
                            }
                        }
                    }
                }

                // Otherwise, assume the requirement is up-to-date.
                Self::Satisfied
            }
            RequirementSource::Git {
                url: _,
                git: requested_git,
                subdirectory: requested_subdirectory,
            } => {
                let InstalledDist::Url(InstalledDirectUrlDist { direct_url, .. }) = &distribution
                else {
                    return Self::Mismatch;
                };
                let DirectUrl::VcsUrl {
                    url: installed_url,
                    vcs_info:
                        VcsInfo {
                            vcs: VcsKind::Git,
                            requested_revision: _,
                            commit_id: installed_precise,
                        },
                    subdirectory: installed_subdirectory,
                } = direct_url.as_ref()
                else {
                    return Self::Mismatch;
                };

                if requested_subdirectory != installed_subdirectory {
                    debug!(
                        "Subdirectory mismatch: {:?} vs. {:?}",
                        installed_subdirectory, requested_subdirectory
                    );
                    return Self::Mismatch;
                }

                if !RepositoryUrl::parse(installed_url).is_ok_and(|installed_url| {
                    installed_url == RepositoryUrl::new(requested_git.repository())
                }) {
                    debug!(
                        "Repository mismatch: {:?} vs. {:?}",
                        installed_url,
                        requested_git.repository()
                    );
                    return Self::Mismatch;
                }

                // TODO(charlie): It would be more consistent for us to compare the requested
                // revisions here.
                if installed_precise.as_deref()
                    != requested_git.precise().as_ref().map(GitOid::as_str)
                {
                    debug!(
                        "Precise mismatch: {:?} vs. {:?}",
                        installed_precise,
                        requested_git.precise()
                    );
                    return Self::OutOfDate;
                }

                Self::Satisfied
            }
            RequirementSource::Path {
                install_path: requested_path,
                ext: _,
                url: _,
            } => {
                let InstalledDist::Url(InstalledDirectUrlDist {
                    direct_url,
                    cache_info,
                    ..
                }) = &distribution
                else {
                    return Self::Mismatch;
                };
                let DirectUrl::ArchiveUrl {
                    url: installed_url,
                    archive_info: _,
                    subdirectory: None,
                } = direct_url.as_ref()
                else {
                    return Self::Mismatch;
                };

                let Some(installed_path) = Url::parse(installed_url)
                    .ok()
                    .and_then(|url| url.to_file_path().ok())
                else {
                    return Self::Mismatch;
                };

                if !(**requested_path == installed_path
                    || is_same_file(requested_path, &installed_path).unwrap_or(false))
                {
                    trace!(
                        "Path mismatch: {:?} vs. {:?}",
                        requested_path, installed_path,
                    );
                    return Self::Mismatch;
                }

                let Some(cache_info) = cache_info.as_ref() else {
                    return Self::OutOfDate;
                };
                match CacheInfo::from_path(requested_path) {
                    Ok(read_cache_info) => {
                        if *cache_info != read_cache_info {
                            return Self::OutOfDate;
                        }
                    }
                    Err(err) => {
                        debug!("Failed to read cached requirement for: {distribution} ({err})");
                        return Self::CacheInvalid;
                    }
                }

                Self::Satisfied
            }
            RequirementSource::Directory {
                install_path: requested_path,
                editable: requested_editable,
                r#virtual: _,
                url: _,
            } => {
                let InstalledDist::Url(InstalledDirectUrlDist {
                    direct_url,
                    cache_info,
                    ..
                }) = &distribution
                else {
                    return Self::Mismatch;
                };
                let DirectUrl::LocalDirectory {
                    url: installed_url,
                    dir_info:
                        DirInfo {
                            editable: installed_editable,
                        },
                    subdirectory: None,
                } = direct_url.as_ref()
                else {
                    return Self::Mismatch;
                };

                if *requested_editable != installed_editable.unwrap_or_default() {
                    trace!(
                        "Editable mismatch: {:?} vs. {:?}",
                        *requested_editable,
                        installed_editable.unwrap_or_default()
                    );
                    return Self::Mismatch;
                }

                let Some(installed_path) = Url::parse(installed_url)
                    .ok()
                    .and_then(|url| url.to_file_path().ok())
                else {
                    return Self::Mismatch;
                };

                if !(**requested_path == installed_path
                    || is_same_file(requested_path, &installed_path).unwrap_or(false))
                {
                    trace!(
                        "Path mismatch: {:?} vs. {:?}",
                        requested_path, installed_path,
                    );
                    return Self::Mismatch;
                }

                let Some(cache_info) = cache_info.as_ref() else {
                    return Self::OutOfDate;
                };
                match CacheInfo::from_path(requested_path) {
                    Ok(read_cache_info) => {
                        if *cache_info != read_cache_info {
                            return Self::OutOfDate;
                        }
                    }
                    Err(err) => {
                        debug!("Failed to read cached requirement for: {distribution} ({err})");
                        return Self::CacheInvalid;
                    }
                }

                Self::Satisfied
            }
        }
    }
}
