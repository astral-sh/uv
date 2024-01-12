use distribution_filename::DistFilename;
use distribution_types::{
    BuiltDist, Dist, PathBuiltDist, PathSourceDist, PrioritizedDistribution, SourceDist,
};
use pep440_rs::Version;
use pep508_rs::VerbatimUrl;
use platform_tags::Tags;
use puffin_normalize::PackageName;
use rustc_hash::FxHashMap;
use std::collections::btree_map::Entry;
use std::collections::BTreeMap;
use std::path::PathBuf;
use tracing::instrument;

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
    pub fn from_dists(
        dists: Vec<(DistFilename, PathBuf)>,
        tags: &Tags,
    ) -> FxHashMap<PackageName, Self> {
        // If we have packages of the same name from find links, gives them priority, otherwise start empty
        let mut flat_index: FxHashMap<PackageName, Self> = FxHashMap::default();

        // Collect compatible distributions.
        for (filename, path) in dists {
            let version_map = flat_index.entry(filename.name().clone()).or_default();

            // No requires python here: For source distributions we don't have that information, for wheels we read it
            // lazily only when they are selected.
            match filename {
                DistFilename::WheelFilename(filename) => {
                    let priority = filename.compatibility(tags);
                    let version = filename.version.clone();
                    let dist = Dist::Built(BuiltDist::Path(PathBuiltDist {
                        filename,
                        url: VerbatimUrl::from_path(&path, path.display().to_string())
                            .expect("Find link paths must be absolute"),
                        path,
                    }));
                    match version_map.0.entry(version.into()) {
                        Entry::Occupied(mut entry) => {
                            entry.get_mut().insert_built(dist, None, priority);
                        }
                        Entry::Vacant(entry) => {
                            entry.insert(PrioritizedDistribution::from_built(dist, None, priority));
                        }
                    }
                }
                DistFilename::SourceDistFilename(filename) => {
                    let dist = Dist::Source(SourceDist::Path(PathSourceDist {
                        name: filename.name,
                        url: VerbatimUrl::from_path(&path, path.display().to_string())
                            .expect("Find link paths must be absolute"),
                        path,
                        editable: false,
                    }));
                    match version_map.0.entry(filename.version.clone().into()) {
                        Entry::Occupied(mut entry) => {
                            entry.get_mut().insert_source(dist, None);
                        }
                        Entry::Vacant(entry) => {
                            entry.insert(PrioritizedDistribution::from_source(dist, None));
                        }
                    }
                }
            }
        }

        flat_index
    }

    pub fn iter(&self) -> impl Iterator<Item = (&T, &PrioritizedDistribution)> {
        self.0.iter()
    }
}
