use std::ops::Deref;

use anyhow::Result;
use tracing::trace;

use distribution_types::{InstalledDirectUrlDist, InstalledDist, UvSource};
use pypi_types::{DirInfo, DirectUrl, VcsInfo, VcsKind};
use uv_cache::{ArchiveTarget, ArchiveTimestamp};

#[derive(Debug, Copy, Clone)]
pub(crate) enum RequirementSatisfaction {
    Mismatch,
    Satisfied,
    OutOfDate,
}

/// Returns true if a requirement is satisfied by an installed distribution.
///
/// Returns an error if IO fails during a freshness check for a local path.
pub(crate) fn installed_satisfies_requirement(
    distribution: &InstalledDist,
    source: &UvSource,
) -> Result<RequirementSatisfaction> {
    trace!(
        "Comparing installed with source: {:?} {:?}",
        distribution,
        source
    );
    // Filter out already-installed packages.
    match source {
        // If the requirement comes from a registry, check by name.
        UvSource::Registry { version, .. } => {
            if version.contains(distribution.version()) {
                return Ok(RequirementSatisfaction::Satisfied);
            }
            Ok(RequirementSatisfaction::Mismatch)
        }
        UvSource::Url {
            url: requested_url,
            subdirectory: requested_subdirectory,
        } => {
            let InstalledDist::Url(InstalledDirectUrlDist {
                direct_url,
                editable,
                ..
            }) = &distribution
            else {
                return Ok(RequirementSatisfaction::Mismatch);
            };
            let DirectUrl::ArchiveUrl {
                url: installed_url,
                archive_info: _,
                subdirectory: installed_subdirectory,
            } = direct_url.as_ref()
            else {
                return Ok(RequirementSatisfaction::Mismatch);
            };

            if *editable {
                return Ok(RequirementSatisfaction::Mismatch);
            }

            if &requested_url.deref().to_string() != installed_url
                || requested_subdirectory != installed_subdirectory
            {
                return Ok(RequirementSatisfaction::Mismatch);
            }

            // If the requirement came from a local path, check freshness.
            if requested_url.scheme() == "file" {
                if let Ok(archive) = requested_url.to_file_path() {
                    if !ArchiveTimestamp::up_to_date_with(
                        &archive,
                        ArchiveTarget::Install(distribution),
                    )? {
                        return Ok(RequirementSatisfaction::OutOfDate);
                    }
                }
            }

            // Otherwise, assume the requirement is up-to-date.
            Ok(RequirementSatisfaction::Satisfied)
        }
        UvSource::Git {
            url: _,
            repository: requested_repository,
            reference: requested_reference,
            subdirectory: requested_subdirectory,
        } => {
            let InstalledDist::Url(InstalledDirectUrlDist { direct_url, .. }) = &distribution
            else {
                return Ok(RequirementSatisfaction::Mismatch);
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
                return Ok(RequirementSatisfaction::Mismatch);
            };
            if &requested_repository.to_string() != installed_url
                || requested_subdirectory != installed_subdirectory
            {
                return Ok(RequirementSatisfaction::Mismatch);
            }
            if installed_reference.as_deref() != requested_reference.as_str() {
                return Ok(RequirementSatisfaction::OutOfDate);
            }

            Ok(RequirementSatisfaction::Satisfied)
        }
        UvSource::Path {
            path,
            url: requested_url,
            editable: requested_editable,
        } => {
            let InstalledDist::Url(InstalledDirectUrlDist { direct_url, .. }) = &distribution
            else {
                return Ok(RequirementSatisfaction::Mismatch);
            };
            let DirectUrl::LocalDirectory {
                url: installed_url,
                dir_info:
                    DirInfo {
                        editable: installed_editable,
                    },
            } = direct_url.as_ref()
            else {
                return Ok(RequirementSatisfaction::Mismatch);
            };

            if &requested_url.to_string() != installed_url
                || requested_editable.unwrap_or_default() != installed_editable.unwrap_or_default()
            {
                return Ok(RequirementSatisfaction::Mismatch);
            }

            if !ArchiveTimestamp::up_to_date_with(path, ArchiveTarget::Install(distribution))? {
                return Ok(RequirementSatisfaction::OutOfDate);
            }

            // Otherwise, assume the requirement is up-to-date.
            Ok(RequirementSatisfaction::Satisfied)
        }
    }
}
