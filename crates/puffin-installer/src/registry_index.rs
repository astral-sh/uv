use std::collections::{BTreeMap, HashMap};

use fs_err as fs;
use tracing::warn;

use distribution_types::{CachedRegistryDist, Metadata};
use pep440_rs::Version;
use platform_tags::Tags;
use puffin_cache::{Cache, CacheBucket, WheelCache};
use puffin_normalize::PackageName;
use pypi_types::IndexUrls;

/// A local index of distributions that originate from a registry, like `PyPI`.
#[derive(Debug, Default)]
pub struct RegistryIndex(HashMap<PackageName, BTreeMap<Version, CachedRegistryDist>>);

impl RegistryIndex {
    /// Build an index of cached distributions from a directory.
    pub fn try_from_directory(cache: &Cache, tags: &Tags, index_urls: &IndexUrls) -> Self {
        let mut index: HashMap<PackageName, BTreeMap<Version, CachedRegistryDist>> = HashMap::new();

        for index_url in index_urls {
            let wheel_dir = cache
                .bucket(CacheBucket::Wheels)
                .join(WheelCache::Index(index_url).wheel_dir());

            let Ok(dir) = wheel_dir.read_dir() else {
                continue;
            };

            for entry in dir {
                let path = match entry.map(|entry| entry.path()) {
                    Ok(path) => path,
                    Err(err) => {
                        warn!(
                            "Failed to read entry of cache at {}: {}",
                            cache.root().display(),
                            err
                        );
                        continue;
                    }
                };

                // Ignore zipped wheels, which represent intermediary cached artifacts.
                if !path.is_dir() {
                    continue;
                }

                match CachedRegistryDist::try_from_path(&path) {
                    Ok(None) => {}
                    Ok(Some(dist_info)) => {
                        // Pick the wheel with the highest priority
                        let compatibility = dist_info.filename.compatibility(tags);
                        if let Some(existing) = index
                            .get_mut(dist_info.name())
                            .and_then(|package| package.get_mut(&dist_info.filename.version))
                        {
                            // Override if we have better compatibility
                            if compatibility > existing.filename.compatibility(tags) {
                                *existing = dist_info;
                            }
                        } else if compatibility.is_some() {
                            index
                                .entry(dist_info.name().clone())
                                .or_default()
                                .insert(dist_info.filename.version.clone(), dist_info);
                        }
                    }
                    Err(err) => {
                        warn!("Invalid cache entry at {}, removing. {err}", path.display());
                        let result = fs::remove_dir_all(&path);
                        if let Err(err) = result {
                            warn!(
                                "Failed to remove invalid cache entry at {}: {err}",
                                path.display()
                            );
                        }
                    }
                }
            }
        }

        Self(index)
    }

    /// Returns a distribution from the index, if it exists.
    pub fn by_name(
        &self,
        name: &PackageName,
    ) -> impl Iterator<Item = (&Version, &CachedRegistryDist)> {
        // Using static to extend the lifetime
        static DEFAULT_MAP: BTreeMap<Version, CachedRegistryDist> = BTreeMap::new();
        // We should only query this
        self.0.get(name).unwrap_or(&DEFAULT_MAP).iter().rev()
    }
}
