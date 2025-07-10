use rustc_hash::FxHashMap;

use uv_distribution_types::{CompatibleDist, ResolvedDist};
use uv_normalize::PackageName;

use crate::candidate_selector::Candidate;

/// A set of package versions pinned to specific files.
///
/// For example, given `Flask==3.0.0`, the [`FilePins`] would contain a mapping from `Flask` to
/// `3.0.0` to the specific wheel or source distribution archive that was pinned for that version.
#[derive(Clone, Debug, Default)]
pub(crate) struct FilePins(FxHashMap<(PackageName, uv_pep440::Version), ResolvedDist>);

// Inserts are common (every time we select a version) while reads are rare (converting the
// final resolution).
impl FilePins {
    /// Pin a candidate package.
    pub(crate) fn insert(&mut self, candidate: &Candidate, dist: &CompatibleDist) {
        self.0
            .entry((candidate.name().clone(), candidate.version().clone()))
            // Avoid the expensive clone when a version is selected again.
            .or_insert_with(|| dist.for_installation().to_owned());
    }

    /// Return the pinned file for the given package name and version, if it exists.
    pub(crate) fn get(
        &self,
        name: &PackageName,
        version: &uv_pep440::Version,
    ) -> Option<&ResolvedDist> {
        self.0.get(&(name.clone(), version.clone()))
    }
}
