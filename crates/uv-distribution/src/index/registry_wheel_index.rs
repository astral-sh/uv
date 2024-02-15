use std::collections::hash_map::Entry;
use std::collections::BTreeMap;
use std::path::Path;

use rustc_hash::FxHashMap;

use distribution_types::{CachedRegistryDist, FlatIndexLocation, IndexLocations, IndexUrl};
use pep440_rs::Version;
use platform_tags::Tags;
use uv_cache::{Cache, CacheBucket, WheelCache};
use uv_fs::{directories, symlinks};
use uv_normalize::PackageName;

use crate::index::cached_wheel::CachedWheel;
use crate::source::{read_http_manifest, MANIFEST};

/// A local index of distributions that originate from a registry, like `PyPI`.
#[derive(Debug)]
pub struct RegistryWheelIndex<'a> {
    cache: &'a Cache,
    tags: &'a Tags,
    index_locations: &'a IndexLocations,
    index: FxHashMap<&'a PackageName, BTreeMap<Version, CachedRegistryDist>>,
}

impl<'a> RegistryWheelIndex<'a> {
    /// Initialize an index of cached distributions from a directory.
    pub fn new(cache: &'a Cache, tags: &'a Tags, index_locations: &'a IndexLocations) -> Self {
        Self {
            cache,
            tags,
            index_locations,
            index: FxHashMap::default(),
        }
    }

    /// Return an iterator over available wheels for a given package.
    ///
    /// If the package is not yet indexed, this will index the package by reading from the cache.
    pub fn get(
        &mut self,
        name: &'a PackageName,
    ) -> impl Iterator<Item = (&Version, &CachedRegistryDist)> {
        self.get_impl(name).iter().rev()
    }

    /// Get the best wheel for the given package name and version.
    ///
    /// If the package is not yet indexed, this will index the package by reading from the cache.
    pub fn get_version(
        &mut self,
        name: &'a PackageName,
        version: &Version,
    ) -> Option<&CachedRegistryDist> {
        self.get_impl(name).get(version)
    }

    /// Get an entry in the index.
    fn get_impl(&mut self, name: &'a PackageName) -> &BTreeMap<Version, CachedRegistryDist> {
        let versions = match self.index.entry(name) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => entry.insert(Self::index(
                name,
                self.cache,
                self.tags,
                self.index_locations,
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
    ) -> BTreeMap<Version, CachedRegistryDist> {
        let mut versions = BTreeMap::new();

        // Collect into owned `IndexUrl`
        let flat_index_urls: Vec<IndexUrl> = index_locations
            .flat_index()
            .filter_map(|flat_index| match flat_index {
                FlatIndexLocation::Path(_) => None,
                FlatIndexLocation::Url(url) => Some(IndexUrl::Url(url.clone())),
            })
            .collect();

        for index_url in index_locations.indexes().chain(flat_index_urls.iter()) {
            // Index all the wheels that were downloaded directly from the registry.
            let wheel_dir = cache.shard(
                CacheBucket::Wheels,
                WheelCache::Index(index_url).remote_wheel_dir(package.to_string()),
            );

            Self::add_directory(&wheel_dir, tags, &mut versions);

            // Index all the built wheels, created by downloading and building source distributions
            // from the registry.
            let cache_shard = cache.shard(
                CacheBucket::BuiltWheels,
                WheelCache::Index(index_url).built_wheel_dir(package.to_string()),
            );

            // For registry wheels, the cache structure is: `<index>/<package-name>/<version>/`.
            for shard in directories(&cache_shard) {
                // Read the existing metadata from the cache, if it exists.
                let cache_shard = cache_shard.shard(shard);
                let manifest_entry = cache_shard.entry(MANIFEST);
                if let Ok(Some(manifest)) = read_http_manifest(&manifest_entry) {
                    Self::add_directory(cache_shard.join(manifest.id()), tags, &mut versions);
                };
            }
        }

        versions
    }

    /// Add the wheels in a given directory to the index.
    ///
    /// Each subdirectory in the given path is expected to be that of an unzipped wheel.
    fn add_directory(
        path: impl AsRef<Path>,
        tags: &Tags,
        versions: &mut BTreeMap<Version, CachedRegistryDist>,
    ) {
        // Unzipped wheels are stored as symlinks into the archive directory.
        for wheel_dir in symlinks(path.as_ref()) {
            match CachedWheel::from_path(&wheel_dir) {
                None => {}
                Some(dist_info) => {
                    let dist_info = dist_info.into_registry_dist();

                    // Pick the wheel with the highest priority
                    let compatibility = dist_info.filename.compatibility(tags);
                    if let Some(existing) = versions.get_mut(&dist_info.filename.version) {
                        // Override if we have better compatibility
                        if compatibility > existing.filename.compatibility(tags) {
                            *existing = dist_info;
                        }
                    } else if compatibility.is_compatible() {
                        versions.insert(dist_info.filename.version.clone(), dist_info);
                    }
                }
            }
        }
    }
}
