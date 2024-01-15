use std::collections::btree_map::Entry;
use std::collections::BTreeMap;

use rustc_hash::FxHashMap;
use tracing::instrument;

use distribution_filename::DistFilename;
use distribution_types::{
    BuiltDist, Dist, File, IndexUrl, PrioritizedDistribution, RegistryBuiltDist,
    RegistrySourceDist, SourceDist,
};
use pep440_rs::Version;
use platform_tags::Tags;
use puffin_normalize::PackageName;

pub type FlatIndexEntry = (DistFilename, File, IndexUrl);

/// A set of [`PrioritizedDistribution`] from a `--find-links` entry, indexed by [`PackageName`]
/// and [`Version`].
#[derive(Debug, Clone, Default)]
pub struct FlatIndex(FxHashMap<PackageName, FlatDistributions>);

impl FlatIndex {
    /// Collect all files from a `--find-links` target into a [`FlatIndex`].
    #[instrument(skip_all)]
    pub fn from_files(dists: Vec<FlatIndexEntry>, tags: &Tags) -> Self {
        let mut flat_index = FxHashMap::default();

        // Collect compatible distributions.
        for (filename, file, index) in dists {
            let distributions = flat_index.entry(filename.name().clone()).or_default();
            Self::add_file(distributions, file, filename, tags, index);
        }

        Self(flat_index)
    }

    fn add_file(
        distributions: &mut FlatDistributions,
        file: File,
        filename: DistFilename,
        tags: &Tags,
        index: IndexUrl,
    ) {
        // No `requires-python` here: for source distributions, we don't have that information;
        // for wheels, we read it lazily only when selected.
        match filename {
            DistFilename::WheelFilename(filename) => {
                let priority = filename.compatibility(tags);
                let version = filename.version.clone();

                let dist = Dist::Built(BuiltDist::Registry(RegistryBuiltDist {
                    filename,
                    file,
                    index,
                }));
                match distributions.0.entry(version) {
                    Entry::Occupied(mut entry) => {
                        entry.get_mut().insert_built(dist, None, None, priority);
                    }
                    Entry::Vacant(entry) => {
                        entry.insert(PrioritizedDistribution::from_built(
                            dist, None, None, priority,
                        ));
                    }
                }
            }
            DistFilename::SourceDistFilename(filename) => {
                let dist = Dist::Source(SourceDist::Registry(RegistrySourceDist {
                    filename: filename.clone(),
                    file,
                    index,
                }));
                match distributions.0.entry(filename.version.clone()) {
                    Entry::Occupied(mut entry) => {
                        entry.get_mut().insert_source(dist, None, None);
                    }
                    Entry::Vacant(entry) => {
                        entry.insert(PrioritizedDistribution::from_source(dist, None, None));
                    }
                }
            }
        }
    }

    /// Get the [`FlatDistributions`] for the given package name.
    pub fn get(&self, package_name: &PackageName) -> Option<&FlatDistributions> {
        self.0.get(package_name)
    }
}

/// A set of [`PrioritizedDistribution`] from a `--find-links` entry for a single package, indexed
/// by [`Version`].
#[derive(Debug, Clone, Default)]
pub struct FlatDistributions(BTreeMap<Version, PrioritizedDistribution>);

impl FlatDistributions {
    pub fn iter(&self) -> impl Iterator<Item = (&Version, &PrioritizedDistribution)> {
        self.0.iter()
    }
}

impl From<FlatDistributions> for BTreeMap<Version, PrioritizedDistribution> {
    fn from(distributions: FlatDistributions) -> Self {
        distributions.0
    }
}
