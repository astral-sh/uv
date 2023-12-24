use rustc_hash::FxHashMap;

use distribution_types::{File, IndexUrl};
use puffin_normalize::PackageName;
use pypi_types::BaseUrl;

use crate::candidate_selector::Candidate;

/// A set of package versions pinned to specific files.
///
/// For example, given `Flask==3.0.0`, the [`FilePins`] would contain a mapping from `Flask` to
/// `3.0.0` to the specific wheel or source distribution archive that was pinned for that version.
#[derive(Debug, Default)]
pub(crate) struct FilePins(
    FxHashMap<PackageName, FxHashMap<pep440_rs::Version, (IndexUrl, BaseUrl, File)>>,
);

impl FilePins {
    /// Pin a candidate package.
    pub(crate) fn insert(&mut self, candidate: &Candidate, index: &IndexUrl, base: &BaseUrl) {
        self.0.entry(candidate.name().clone()).or_default().insert(
            candidate.version().clone().into(),
            (
                index.clone(),
                base.clone(),
                candidate.install().clone().into(),
            ),
        );
    }

    /// Return the pinned file for the given package name and version, if it exists.
    pub(crate) fn get(
        &self,
        name: &PackageName,
        version: &pep440_rs::Version,
    ) -> Option<&(IndexUrl, BaseUrl, File)> {
        self.0.get(name)?.get(version)
    }
}
