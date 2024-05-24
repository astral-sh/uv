use rustc_hash::FxHashMap;

use distribution_types::{CompatibleDist, ResolvedDist};
use uv_normalize::PackageName;

use crate::candidate_selector::Candidate;

/// A set of package versions pinned to specific files.
///
/// For example, given `Flask==3.0.0`, the [`FilePins`] would contain a mapping from `Flask` to
/// `3.0.0` to the specific wheel or source distribution archive that was pinned for that version.
#[derive(Clone, Debug, Default)]
pub(crate) struct FilePins(FxHashMap<PackageName, FxHashMap<pep440_rs::Version, ResolvedDist>>);

impl FilePins {
    /// Pin a candidate package.
    pub(crate) fn insert(&mut self, candidate: &Candidate, dist: &CompatibleDist) {
        self.0.entry(candidate.name().clone()).or_default().insert(
            candidate.version().clone(),
            dist.for_installation().to_owned(),
        );
    }

    /// Return the pinned file for the given package name and version, if it exists.
    pub(crate) fn get(
        &self,
        name: &PackageName,
        version: &pep440_rs::Version,
    ) -> Option<&ResolvedDist> {
        self.0.get(name)?.get(version)
    }

    /// Add the pins in `other` to `self`.
    ///
    /// This assumes that if a version for a particular package exists in
    /// both `self` and `other`, then they will both correspond to identical
    /// distributions.
    pub(crate) fn union(&mut self, other: FilePins) {
        for (name, versions) in other.0 {
            self.0.entry(name).or_default().extend(versions);
        }
    }
}
