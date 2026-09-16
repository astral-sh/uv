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
    hash_source: PinnedHashSource,
}

/// The source of hashes emitted for a pinned distribution.
#[derive(Debug, Clone, Copy)]
pub enum PinnedHashSource {
    /// Reuse hashes collected during resolution, including existing requirements hashes.
    Package,
    /// Derive hashes from the retained registry files, hashing files that lack advertised hashes.
    Artifacts,
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
            hash_source: PinnedHashSource::Package,
        })
    }

    pub fn hash_source(&self) -> PinnedHashSource {
        self.hash_source
    }
}

impl From<&CompatibleDist<'_>> for PinnedDist {
    /// Pin the installation artifact, not a wheel used only to read metadata.
    fn from(dist: &CompatibleDist<'_>) -> Self {
        Self {
            dist: dist.for_installation().to_owned(),
            hash_source: dist
                .prioritized()
                .map_or(PinnedHashSource::Package, PrioritizedDist::hash_source),
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
