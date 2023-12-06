use std::collections::BTreeMap;
use std::path::Path;

use fs_err as fs;
use fxhash::FxHashMap;
use tracing::warn;

use distribution_types::{CachedRegistryDist, CachedWheel, Metadata};
use pep440_rs::Version;
use platform_tags::Tags;
use puffin_cache::{Cache, CacheBucket, WheelCache};
use puffin_normalize::PackageName;
use pypi_types::IndexUrls;

use crate::index::iter_directories;

/// A local index of distributions that originate from a registry, like `PyPI`.
#[derive(Debug, Default)]
pub struct RegistryWheelIndex(FxHashMap<PackageName, BTreeMap<Version, CachedRegistryDist>>);

impl RegistryWheelIndex {
    /// Build an index of cached distributions from a directory.
    pub fn from_directory(cache: &Cache, tags: &Tags, index_urls: &IndexUrls) -> Self {
        let mut index = Self::default();

        for index_url in index_urls {
            // Index all the wheels that were downloaded directly from the registry.
            // TODO(charlie): Shard the cache by package name, and do this lazily.
            let wheel_dir = cache
                .bucket(CacheBucket::Wheels)
                .join(WheelCache::Index(index_url).wheel_dir());

            index.add_directory(wheel_dir, tags);

            // Index all the built wheels, created by downloading and building source distributions
            // from the registry.
            // TODO(charlie): Shard the cache by package name, and do this lazily.
            let built_wheel_dir = cache
                .bucket(CacheBucket::BuiltWheels)
                .join(WheelCache::Index(index_url).wheel_dir());

            let Ok(read_dir) = built_wheel_dir.read_dir() else {
                continue;
            };
            for subdir in iter_directories(read_dir) {
                index.add_directory(subdir, tags);
            }
        }

        index
    }

    /// Returns a distribution from the index, if it exists.
    pub fn by_name(
        &self,
        name: &PackageName,
    ) -> impl Iterator<Item = (&Version, &CachedRegistryDist)> {
        // Using static to extend the lifetime
        static DEFAULT_MAP: BTreeMap<Version, CachedRegistryDist> = BTreeMap::new();
        self.0.get(name).unwrap_or(&DEFAULT_MAP).iter().rev()
    }

    /// Add the wheels in a given directory to the index.
    ///
    /// Each subdirectory in the given path is expected to be that of an unzipped wheel.
    fn add_directory(&mut self, path: impl AsRef<Path>, tags: &Tags) {
        let Ok(read_dir) = path.as_ref().read_dir() else {
            return;
        };

        for wheel_dir in iter_directories(read_dir) {
            match CachedWheel::from_path(&wheel_dir) {
                Ok(None) => {}
                Ok(Some(dist_info)) => {
                    let dist_info = dist_info.into_registry_dist();

                    // Pick the wheel with the highest priority
                    let compatibility = dist_info.filename.compatibility(tags);
                    if let Some(existing) = self
                        .0
                        .get_mut(dist_info.name())
                        .and_then(|package| package.get_mut(&dist_info.filename.version))
                    {
                        // Override if we have better compatibility
                        if compatibility > existing.filename.compatibility(tags) {
                            *existing = dist_info;
                        }
                    } else if compatibility.is_some() {
                        self.0
                            .entry(dist_info.name().clone())
                            .or_default()
                            .insert(dist_info.filename.version.clone(), dist_info);
                    }
                }
                Err(err) => {
                    warn!(
                        "Invalid cache entry at {}, removing. {err}",
                        wheel_dir.display()
                    );
                    let result = fs::remove_dir_all(&wheel_dir);
                    if let Err(err) = result {
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
