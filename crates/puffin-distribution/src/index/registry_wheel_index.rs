use std::collections::hash_map::Entry;
use std::collections::BTreeMap;
use std::path::Path;

use fs_err as fs;
use rustc_hash::FxHashMap;
use tracing::warn;

use distribution_types::{CachedRegistryDist, CachedWheel};
use pep440_rs::Version;
use platform_tags::Tags;
use puffin_cache::{Cache, CacheBucket, WheelCache};
use puffin_fs::directories;
use puffin_normalize::PackageName;
use pypi_types::IndexUrls;

/// A local index of distributions that originate from a registry, like `PyPI`.
#[derive(Debug)]
pub struct RegistryWheelIndex<'a> {
    cache: &'a Cache,
    tags: &'a Tags,
    index_urls: &'a IndexUrls,
    index: FxHashMap<PackageName, BTreeMap<Version, CachedRegistryDist>>,
}

impl<'a> RegistryWheelIndex<'a> {
    /// Initialize an index of cached distributions from a directory.
    pub fn new(cache: &'a Cache, tags: &'a Tags, index_urls: &'a IndexUrls) -> Self {
        Self {
            cache,
            tags,
            index_urls,
            index: FxHashMap::default(),
        }
    }

    /// Return an iterator over available wheels for a given package.
    ///
    /// If the package is not yet indexed, this will index the package by reading from the cache.
    pub fn get(
        &mut self,
        name: &PackageName,
    ) -> impl Iterator<Item = (&Version, &CachedRegistryDist)> {
        let versions = match self.index.entry(name.clone()) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => {
                entry.insert(Self::index(name, self.cache, self.tags, self.index_urls))
            }
        };
        versions.iter().rev()
    }

    /// Add a package to the index by reading from the cache.
    fn index(
        package: &PackageName,
        cache: &Cache,
        tags: &Tags,
        index_urls: &IndexUrls,
    ) -> BTreeMap<Version, CachedRegistryDist> {
        let mut versions = BTreeMap::new();

        for index_url in index_urls {
            // Index all the wheels that were downloaded directly from the registry.
            let wheel_dir = cache.shard(
                CacheBucket::Wheels,
                WheelCache::Index(index_url).remote_wheel_dir(package.to_string()),
            );

            Self::add_directory(&*wheel_dir, tags, &mut versions);

            // Index all the built wheels, created by downloading and building source distributions
            // from the registry.
            let built_wheel_dir = cache.shard(
                CacheBucket::BuiltWheels,
                WheelCache::Index(index_url).built_wheel_dir(package.to_string()),
            );

            // Built wheels have one more level of indirection, as they are keyed by the source
            // distribution filename.
            for subdir in directories(&*built_wheel_dir) {
                Self::add_directory(subdir, tags, &mut versions);
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
        for wheel_dir in directories(path.as_ref()) {
            match CachedWheel::from_path(&wheel_dir) {
                Ok(None) => {}
                Ok(Some(dist_info)) => {
                    let dist_info = dist_info.into_registry_dist();

                    // Pick the wheel with the highest priority
                    let compatibility = dist_info.filename.compatibility(tags);
                    if let Some(existing) = versions.get_mut(&dist_info.filename.version) {
                        // Override if we have better compatibility
                        if compatibility > existing.filename.compatibility(tags) {
                            *existing = dist_info;
                        }
                    } else if compatibility.is_some() {
                        versions.insert(dist_info.filename.version.clone(), dist_info);
                    }
                }
                Err(err) => {
                    warn!(
                        "Invalid cache entry at {}, removing. {err}",
                        wheel_dir.display()
                    );

                    if let Err(err) = fs::remove_dir_all(&wheel_dir) {
                        warn!(
                            "Failed to remove invalid cache entry at {}: {err}",
                            wheel_dir.display()
                        );
                    }
                }
            }
        }
    }
}
