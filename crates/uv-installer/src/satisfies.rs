use anyhow::Result;
use std::fmt::Debug;
use tracing::trace;

use distribution_types::InstalledDist;
use pep508_rs::VersionOrUrlRef;

use uv_cache::{ArchiveTarget, ArchiveTimestamp};

#[derive(Debug, Copy, Clone)]
pub(crate) enum RequirementSatisfaction {
    Mismatch,
    Satisfied,
    OutOfDate,
}

impl RequirementSatisfaction {
    /// Returns true if a requirement is satisfied by an installed distribution.
    ///
    /// Returns an error if IO fails during a freshness check for a local path.
    pub(crate) fn check(
        distribution: &InstalledDist,
        version_or_url: Option<VersionOrUrlRef>,
        requirement: impl Debug,
    ) -> Result<Self> {
        trace!(
            "Comparing installed with requirement: {:?} {:?}",
            distribution,
            requirement
        );
        // Filter out already-installed packages.
        match version_or_url {
            // Accept any version of the package.
            None => return Ok(Self::Satisfied),

            // If the requirement comes from a registry, check by name.
            Some(VersionOrUrlRef::VersionSpecifier(version_specifier)) => {
                if version_specifier.contains(distribution.version()) {
                    return Ok(Self::Satisfied);
                }
            }

            // If the requirement comes from a direct URL, check by URL.
            Some(VersionOrUrlRef::Url(url)) => {
                if let InstalledDist::Url(installed) = &distribution {
                    if !installed.editable && &installed.url == url.raw() {
                        // If the requirement came from a local path, check freshness.
                        return if let Some(archive) = (url.scheme() == "file")
                            .then(|| url.to_file_path().ok())
                            .flatten()
                        {
                            if ArchiveTimestamp::up_to_date_with(
                                &archive,
                                ArchiveTarget::Install(distribution),
                            )? {
                                return Ok(Self::Satisfied);
                            }
                            Ok(Self::OutOfDate)
                        } else {
                            // Otherwise, assume the requirement is up-to-date.
                            Ok(Self::Satisfied)
                        };
                    }
                }
            }
        }

        Ok(Self::Mismatch)
    }
}
