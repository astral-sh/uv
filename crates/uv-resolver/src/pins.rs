use rustc_hash::FxHashMap;

use distribution_types::{CompatibleDist, Dist};
use uv_normalize::PackageName;

use crate::candidate_selector::Candidate;

/// A set of package versions pinned to specific files.
///
/// For example, given `Flask==3.0.0`, the [`FilePins`] would contain a mapping from `Flask` to
/// `3.0.0` to the specific wheel or source distribution archive that was pinned for that version.
#[derive(Debug, Default)]
pub(crate) struct FilePins(FxHashMap<PackageName, FxHashMap<pep440_rs::Version, Dist>>);

impl FilePins {
    /// Pin a candidate package.
    pub(crate) fn insert(&mut self, candidate: &Candidate, dist: &CompatibleDist) {
        self.0.entry(candidate.name().clone()).or_default().insert(
            candidate.version().clone(),
            dist.for_installation().dist.clone(),
        );
    }

    /// Return the pinned file for the given package name and version, if it exists.
    pub(crate) fn get(&self, name: &PackageName, version: &pep440_rs::Version) -> Option<&Dist> {
        self.0.get(name)?.get(version)
    }
}
