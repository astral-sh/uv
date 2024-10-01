use std::collections::hash_map::Entry;

use rustc_hash::FxHashMap;

use uv_cache::{Cache, CacheBucket, WheelCache};
use uv_distribution_types::{CachedRegistryDist, Hashed, IndexLocations, IndexUrl};
use uv_fs::{directories, files, symlinks};
use uv_normalize::PackageName;
use uv_platform_tags::Tags;
use uv_types::HashStrategy;

use crate::index::cached_wheel::CachedWheel;
use crate::source::{HttpRevisionPointer, LocalRevisionPointer, HTTP_REVISION, LOCAL_REVISION};

/// An entry in the [`RegistryWheelIndex`].
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct IndexEntry {
    /// The cached distribution.
    pub dist: CachedRegistryDist,
    /// Whether the wheel was built from source (true), or downloaded from the registry directly (false).
    pub built: bool,
}

/// A local index of distributions that originate from a registry, like `PyPI`.
#[derive(Debug)]
pub struct RegistryWheelIndex<'a> {
    cache: &'a Cache,
    tags: &'a Tags,
    index_locations: &'a IndexLocations,
    hasher: &'a HashStrategy,
    index: FxHashMap<&'a PackageName, Vec<IndexEntry>>,
}

impl<'a> RegistryWheelIndex<'a> {
    /// Initialize an index of registry distributions.
    pub fn new(
        cache: &'a Cache,
        tags: &'a Tags,
        index_locations: &'a IndexLocations,
        hasher: &'a HashStrategy,
    ) -> Self {
        Self {
            cache,
            tags,
            index_locations,
            hasher,
            index: FxHashMap::default(),
        }
    }

    /// Return an iterator over available wheels for a given package.
    ///
    /// If the package is not yet indexed, this will index the package by reading from the cache.
    pub fn get(&mut self, name: &'a PackageName) -> impl Iterator<Item = &IndexEntry> {
        self.get_impl(name).iter().rev()
    }

    /// Get an entry in the index.
    fn get_impl(&mut self, name: &'a PackageName) -> &[IndexEntry] {
        let versions = match self.index.entry(name) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => entry.insert(Self::index(
                name,
                self.cache,
                self.tags,
                self.index_locations,
                self.hasher,
            )),
        };
        versions
    }

    /// Add a package to the index by reading from the cache.
    fn index(
        package: &PackageName,
        cache: &Cache,
        tags: &Tags,
        index_locations: &IndexLocations,
        hasher: &HashStrategy,
    ) -> Vec<IndexEntry> {
        let mut entries = vec![];

        // Collect into owned `IndexUrl`.
        let flat_index_urls: Vec<IndexUrl> = index_locations
            .flat_index()
            .map(|flat_index| IndexUrl::from(flat_index.clone()))
            .collect();

        for index_url in index_locations.indexes().chain(flat_index_urls.iter()) {
            // Index all the wheels that were downloaded directly from the registry.
            let wheel_dir = cache.shard(
                CacheBucket::Wheels,
                WheelCache::Index(index_url).wheel_dir(package.to_string()),
            );

            // For registry wheels, the cache structure is: `<index>/<package-name>/<wheel>.http`
            // or `<index>/<package-name>/<version>/<wheel>.rev`.
            for file in files(&wheel_dir) {
                match index_url {
                    // Add files from remote registries.
                    IndexUrl::Pypi(_) | IndexUrl::Url(_) => {
                        if file
                            .extension()
                            .is_some_and(|ext| ext.eq_ignore_ascii_case("http"))
                        {
                            if let Some(wheel) =
                                CachedWheel::from_http_pointer(wheel_dir.join(file), cache)
                            {
                                if wheel.filename.compatibility(tags).is_compatible() {
                                    // Enforce hash-checking based on the built distribution.
                                    if wheel.satisfies(
                                        hasher.get_package(
                                            &wheel.filename.name,
                                            &wheel.filename.version,
                                        ),
                                    ) {
                                        entries.push(IndexEntry {
                                            dist: wheel.into_registry_dist(),
                                            built: false,
                                        });
                                    }
                                }
                            }
                        }
                    }
                    // Add files from local registries (e.g., `--find-links`).
                    IndexUrl::Path(_) => {
                        if file
                            .extension()
                            .is_some_and(|ext| ext.eq_ignore_ascii_case("rev"))
                        {
                            if let Some(wheel) =
                                CachedWheel::from_local_pointer(wheel_dir.join(file), cache)
                            {
                                if wheel.filename.compatibility(tags).is_compatible() {
                                    // Enforce hash-checking based on the built distribution.
                                    if wheel.satisfies(
                                        hasher.get_package(
                                            &wheel.filename.name,
                                            &wheel.filename.version,
                                        ),
                                    ) {
                                        entries.push(IndexEntry {
                                            dist: wheel.into_registry_dist(),
                                            built: false,
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Index all the built wheels, created by downloading and building source distributions
            // from the registry.
            let cache_shard = cache.shard(
                CacheBucket::SourceDistributions,
                WheelCache::Index(index_url).wheel_dir(package.to_string()),
            );

            // For registry wheels, the cache structure is: `<index>/<package-name>/<version>/`.
            for shard in directories(&cache_shard) {
                // Read the existing metadata from the cache, if it exists.
                let cache_shard = cache_shard.shard(shard);

                // Read the revision from the cache.
                let revision = match index_url {
                    // Add files from remote registries.
                    IndexUrl::Pypi(_) | IndexUrl::Url(_) => {
                        let revision_entry = cache_shard.entry(HTTP_REVISION);
                        if let Ok(Some(pointer)) = HttpRevisionPointer::read_from(revision_entry) {
                            Some(pointer.into_revision())
                        } else {
                            None
                        }
                    }
                    // Add files from local registries (e.g., `--find-links`).
                    IndexUrl::Path(_) => {
                        let revision_entry = cache_shard.entry(LOCAL_REVISION);
                        if let Ok(Some(pointer)) = LocalRevisionPointer::read_from(revision_entry) {
                            Some(pointer.into_revision())
                        } else {
                            None
                        }
                    }
                };

                if let Some(revision) = revision {
                    for wheel_dir in symlinks(cache_shard.join(revision.id())) {
                        if let Some(wheel) = CachedWheel::from_built_source(wheel_dir) {
                            if wheel.filename.compatibility(tags).is_compatible() {
                                // Enforce hash-checking based on the source distribution.
                                if revision.satisfies(
                                    hasher
                                        .get_package(&wheel.filename.name, &wheel.filename.version),
                                ) {
                                    entries.push(IndexEntry {
                                        dist: wheel.into_registry_dist(),
                                        built: true,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }

        // Sort the cached distributions by (1) version, (2) compatibility, and (3) build status.
        // We want the highest versions, with the greatest compatibility, that were built from source.
        // at the end of the list.
        entries.sort_unstable_by(|a, b| {
            a.dist
                .filename
                .version
                .cmp(&b.dist.filename.version)
                .then_with(|| {
                    a.dist
                        .filename
                        .compatibility(tags)
                        .cmp(&b.dist.filename.compatibility(tags))
                        .then_with(|| a.built.cmp(&b.built))
                })
        });

        entries
    }
}
