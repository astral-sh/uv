use std::collections::btree_map::Entry;
use std::collections::BTreeMap;

use rustc_hash::FxHashMap;
use tracing::instrument;

use distribution_filename::{DistFilename, SourceDistFilename, WheelFilename};
use distribution_types::{
    File, HashComparison, HashPolicy, IncompatibleSource, IncompatibleWheel, IndexUrl,
    PrioritizedDist, RegistryBuiltWheel, RegistrySourceDist, SourceDistCompatibility,
    WheelCompatibility,
};
use pep440_rs::Version;
use platform_tags::{TagCompatibility, Tags};
use pypi_types::HashDigest;
use uv_client::FlatIndexEntries;
use uv_configuration::BuildOptions;
use uv_normalize::PackageName;
use uv_types::HashStrategy;

/// A set of [`PrioritizedDist`] from a `--find-links` entry, indexed by [`PackageName`]
/// and [`Version`].
#[derive(Debug, Clone, Default)]
pub struct FlatIndex {
    /// The list of [`FlatDistributions`] from the `--find-links` entries, indexed by package name.
    index: FxHashMap<PackageName, FlatDistributions>,
    /// Whether any `--find-links` entries could not be resolved due to a lack of network
    /// connectivity.
    offline: bool,
}

impl FlatIndex {
    /// Collect all files from a `--find-links` target into a [`FlatIndex`].
    #[instrument(skip_all)]
    pub fn from_entries(
        entries: FlatIndexEntries,
        tags: Option<&Tags>,
        hasher: &HashStrategy,
        build_options: &BuildOptions,
    ) -> Self {
        // Collect compatible distributions.
        let mut index = FxHashMap::default();
        for (filename, file, url) in entries.entries {
            let distributions = index.entry(filename.name().clone()).or_default();
            Self::add_file(
                distributions,
                file,
                filename,
                tags,
                hasher,
                build_options,
                url,
            );
        }

        // Collect offline entries.
        let offline = entries.offline;

        Self { index, offline }
    }

    fn add_file(
        distributions: &mut FlatDistributions,
        file: File,
        filename: DistFilename,
        tags: Option<&Tags>,
        hasher: &HashStrategy,
        build_options: &BuildOptions,
        index: IndexUrl,
    ) {
        // No `requires-python` here: for source distributions, we don't have that information;
        // for wheels, we read it lazily only when selected.
        match filename {
            DistFilename::WheelFilename(filename) => {
                let version = filename.version.clone();

                let compatibility =
                    Self::wheel_compatibility(&filename, &file.hashes, tags, hasher, build_options);
                let dist = RegistryBuiltWheel {
                    filename,
                    file: Box::new(file),
                    index,
                };
                match distributions.0.entry(version) {
                    Entry::Occupied(mut entry) => {
                        entry.get_mut().insert_built(dist, vec![], compatibility);
                    }
                    Entry::Vacant(entry) => {
                        entry.insert(PrioritizedDist::from_built(dist, vec![], compatibility));
                    }
                }
            }
            DistFilename::SourceDistFilename(filename) => {
                let compatibility =
                    Self::source_dist_compatibility(&filename, &file.hashes, hasher, build_options);
                let dist = RegistrySourceDist {
                    name: filename.name.clone(),
                    version: filename.version.clone(),
                    ext: filename.extension,
                    file: Box::new(file),
                    index,
                    wheels: vec![],
                };
                match distributions.0.entry(filename.version) {
                    Entry::Occupied(mut entry) => {
                        entry.get_mut().insert_source(dist, vec![], compatibility);
                    }
                    Entry::Vacant(entry) => {
                        entry.insert(PrioritizedDist::from_source(dist, vec![], compatibility));
                    }
                }
            }
        }
    }

    fn source_dist_compatibility(
        filename: &SourceDistFilename,
        hashes: &[HashDigest],
        hasher: &HashStrategy,
        build_options: &BuildOptions,
    ) -> SourceDistCompatibility {
        // Check if source distributions are allowed for this package.
        if build_options.no_build_package(&filename.name) {
            return SourceDistCompatibility::Incompatible(IncompatibleSource::NoBuild);
        }

        // Check if hashes line up
        let hash = if let HashPolicy::Validate(required) =
            hasher.get_package(&filename.name, &filename.version)
        {
            if hashes.is_empty() {
                HashComparison::Missing
            } else if required.iter().any(|hash| hashes.contains(hash)) {
                HashComparison::Matched
            } else {
                HashComparison::Mismatched
            }
        } else {
            HashComparison::Matched
        };

        SourceDistCompatibility::Compatible(hash)
    }

    fn wheel_compatibility(
        filename: &WheelFilename,
        hashes: &[HashDigest],
        tags: Option<&Tags>,
        hasher: &HashStrategy,
        build_options: &BuildOptions,
    ) -> WheelCompatibility {
        // Check if binaries are allowed for this package.
        if build_options.no_binary_package(&filename.name) {
            return WheelCompatibility::Incompatible(IncompatibleWheel::NoBinary);
        }

        // Determine a compatibility for the wheel based on tags.
        let priority = match tags {
            Some(tags) => match filename.compatibility(tags) {
                TagCompatibility::Incompatible(tag) => {
                    return WheelCompatibility::Incompatible(IncompatibleWheel::Tag(tag))
                }
                TagCompatibility::Compatible(priority) => Some(priority),
            },
            None => None,
        };

        // Check if hashes line up.
        let hash = if let HashPolicy::Validate(required) =
            hasher.get_package(&filename.name, &filename.version)
        {
            if hashes.is_empty() {
                HashComparison::Missing
            } else if required.iter().any(|hash| hashes.contains(hash)) {
                HashComparison::Matched
            } else {
                HashComparison::Mismatched
            }
        } else {
            HashComparison::Matched
        };

        // Break ties with the build tag.
        let build_tag = filename.build_tag.clone();

        WheelCompatibility::Compatible(hash, priority, build_tag)
    }

    /// Get the [`FlatDistributions`] for the given package name.
    pub fn get(&self, package_name: &PackageName) -> Option<&FlatDistributions> {
        self.index.get(package_name)
    }

    /// Returns `true` if there are any offline `--find-links` entries.
    pub fn offline(&self) -> bool {
        self.offline
    }
}

/// A set of [`PrioritizedDist`] from a `--find-links` entry for a single package, indexed
/// by [`Version`].
#[derive(Debug, Clone, Default)]
pub struct FlatDistributions(BTreeMap<Version, PrioritizedDist>);

impl FlatDistributions {
    pub fn iter(&self) -> impl Iterator<Item = (&Version, &PrioritizedDist)> {
        self.0.iter()
    }

    pub fn remove(&mut self, version: &Version) -> Option<PrioritizedDist> {
        self.0.remove(version)
    }
}

impl IntoIterator for FlatDistributions {
    type Item = (Version, PrioritizedDist);
    type IntoIter = std::collections::btree_map::IntoIter<Version, PrioritizedDist>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl From<FlatDistributions> for BTreeMap<Version, PrioritizedDist> {
    fn from(distributions: FlatDistributions) -> Self {
        distributions.0
    }
}
