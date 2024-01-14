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

#[derive(Debug, Clone)]
pub struct FlatIndex<T: Into<Version> + From<Version> + Ord>(
    pub BTreeMap<T, PrioritizedDistribution>,
);

impl<T: Into<Version> + From<Version> + Ord> Default for FlatIndex<T> {
    fn default() -> Self {
        Self(BTreeMap::default())
    }
}

impl<T: Into<Version> + From<Version> + Ord> FlatIndex<T> {
    /// Collect all the files from `--find-links` into a override hashmap we can pass into version map creation.
    #[instrument(skip_all)]
    pub fn from_files(
        dists: Vec<(DistFilename, File, IndexUrl)>,
        tags: &Tags,
    ) -> FxHashMap<PackageName, Self> {
        // If we have packages of the same name from find links, gives them priority, otherwise start empty
        let mut flat_index: FxHashMap<PackageName, Self> = FxHashMap::default();

        // Collect compatible distributions.
        for (filename, file, index) in dists {
            let version_map = flat_index.entry(filename.name().clone()).or_default();
            Self::add_file(version_map, file, filename, tags, index);
        }

        flat_index
    }

    fn add_file(
        version_map: &mut FlatIndex<T>,
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
                match version_map.0.entry(version.into()) {
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
                match version_map.0.entry(filename.version.clone().into()) {
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

    pub fn iter(&self) -> impl Iterator<Item = (&T, &PrioritizedDistribution)> {
        self.0.iter()
    }
}
