use std::ops::Deref;
use std::sync::Arc;

use uv_normalize::PackageName;
use uv_pep440::Version;
use uv_pypi_types::VerbatimParsedUrl;

use crate::{CompatibleDist, Dist, Error, PrioritizedDist, ResolvedDist};

/// An installation distribution whose registry artifacts have been filtered during selection.
///
/// Registry pins can only be constructed from a selected candidate. The distribution and its
/// artifact restrictions are immutable, so downstream consumers cannot invalidate the selection.
#[derive(Debug, Clone)]
pub struct PinnedDist {
    dist: ResolvedDist,
    requires_artifact_hashes: bool,
}

impl PinnedDist {
    /// Pin a direct URL, which has no registry artifact set to filter.
    pub fn from_url(
        name: PackageName,
        url: VerbatimParsedUrl,
        version: Version,
    ) -> Result<Self, Error> {
        Ok(Self {
            dist: ResolvedDist::Installable {
                dist: Arc::new(Dist::from_url(name, url)?),
                version: Some(version),
            },
            requires_artifact_hashes: false,
        })
    }

    /// Whether requirements hashes must be derived from the retained registry artifacts instead
    /// of reusing package-level hashes from an earlier resolution.
    pub fn requires_artifact_hashes(&self) -> bool {
        self.requires_artifact_hashes
    }
}

impl From<&CompatibleDist<'_>> for PinnedDist {
    fn from(dist: &CompatibleDist<'_>) -> Self {
        Self {
            dist: dist.for_installation().to_owned(),
            requires_artifact_hashes: dist
                .prioritized()
                .is_some_and(PrioritizedDist::requires_artifact_hashes),
        }
    }
}

impl AsRef<ResolvedDist> for PinnedDist {
    fn as_ref(&self) -> &ResolvedDist {
        &self.dist
    }
}

impl Deref for PinnedDist {
    type Target = ResolvedDist;

    fn deref(&self) -> &Self::Target {
        &self.dist
    }
}
