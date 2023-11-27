use std::collections::HashMap;

use fs_err as fs;
use tracing::warn;

use distribution_types::{CachedRegistryDist, Metadata};
use platform_tags::Tags;
use puffin_cache::Cache;
use puffin_normalize::PackageName;

use crate::cache::{CacheShard, WheelCache};

/// A local index of distributions that originate from a registry, like `PyPI`.
#[derive(Debug, Default)]
pub struct RegistryIndex(HashMap<PackageName, CachedRegistryDist>);

impl RegistryIndex {
    /// Build an index of cached distributions from a directory.
    pub fn try_from_directory(cache: &Cache, tags: &Tags) -> Self {
        let mut index = HashMap::new();

        let cache = WheelCache::new(cache);
        let Ok(dir) = cache.read_dir(CacheShard::Registry) else {
            return Self(index);
        };

        for entry in dir {
            let (path, file_type) =
                match entry.and_then(|entry| Ok((entry.path(), entry.file_type()?))) {
                    Ok((path, file_type)) => (path, file_type),
                    Err(err) => {
                        warn!(
                            "Failed to read entry of cache at {}: {}",
                            cache.root().display(),
                            err
                        );
                        continue;
                    }
                };
            if !file_type.is_dir() {
                continue;
            }

            match CachedRegistryDist::try_from_path(&path) {
                Ok(None) => {}
                Ok(Some(dist_info)) => {
                    // Pick the wheel with the highest priority
                    let compatibility = dist_info.filename.compatibility(tags);
                    if let Some(existing) = index.get_mut(dist_info.name()) {
                        // Override if we have better compatibility
                        if compatibility > existing.filename.compatibility(tags) {
                            *existing = dist_info;
                        }
                    } else if compatibility.is_some() {
                        index.insert(dist_info.name().clone(), dist_info);
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

        Self(index)
    }

    /// Returns a distribution from the index, if it exists.
    pub fn get(&self, name: &PackageName) -> Option<&CachedRegistryDist> {
        self.0.get(name)
    }
}
