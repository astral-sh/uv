use rustc_hash::FxHashMap;

use uv_distribution_types::{CompatibleDist, DistributionId, Identifier, ResolvedDist};
use uv_normalize::PackageName;

use crate::candidate_selector::Candidate;

#[derive(Clone, Debug)]
struct FilePin {
    /// The concrete distribution chosen for installation and locking.
    dist: ResolvedDist,
    /// The concrete distribution whose metadata was used during resolution.
    metadata_id: DistributionId,
}

/// A set of package versions pinned to specific files.
///
/// For example, given `Flask==3.0.0`, the [`FilePins`] would contain a mapping from `Flask` to
/// `3.0.0` to the specific wheel or source distribution archive that was pinned for installation,
/// along with the concrete distribution whose metadata was used during resolution.
#[derive(Clone, Debug, Default)]
pub(crate) struct FilePins(FxHashMap<(PackageName, uv_pep440::Version), FilePin>);

// Inserts are common (every time we select a version) while reads are rare (converting the
// final resolution).
impl FilePins {
    /// Pin a candidate package.
    pub(crate) fn insert(&mut self, candidate: &Candidate, dist: &CompatibleDist) {
        self.0.insert(
            (candidate.name().clone(), candidate.version().clone()),
            FilePin {
                dist: dist.for_installation().to_owned(),
                metadata_id: dist.for_resolution().distribution_id(),
            },
        );
    }

    /// Return the pinned file for the given package name and version, if it exists.
    pub(crate) fn get(
        &self,
        name: &PackageName,
        version: &uv_pep440::Version,
    ) -> Option<&ResolvedDist> {
        self.0
            .get(&(name.clone(), version.clone()))
            .map(|pin| &pin.dist)
    }

    /// Return the pinned distribution and its metadata id in a single lookup.
    pub(crate) fn dist_and_id(
        &self,
        name: &PackageName,
        version: &uv_pep440::Version,
    ) -> Option<(&ResolvedDist, &DistributionId)> {
        self.0
            .get(&(name.clone(), version.clone()))
            .map(|pin| (&pin.dist, &pin.metadata_id))
    }
}
