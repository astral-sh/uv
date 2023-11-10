use std::collections::HashMap;
use std::path::Path;

use fs_err as fs;
use tracing::warn;

use puffin_distribution::{BaseDistribution, CachedRegistryDistribution};
use puffin_normalize::PackageName;

use crate::cache::{CacheShard, WheelCache};

/// A local index of distributions that originate from a registry, like `PyPI`.
#[derive(Debug, Default)]
pub struct RegistryIndex(HashMap<PackageName, CachedRegistryDistribution>);

impl RegistryIndex {
    /// Build an index of cached distributions from a directory.
    pub fn try_from_directory(path: &Path) -> Self {
        let mut index = HashMap::new();

        let cache = WheelCache::new(path);
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
                            path.display(),
                            err
                        );
                        continue;
                    }
                };
            if file_type.is_dir() {
                match CachedRegistryDistribution::try_from_path(&path) {
                    Ok(None) => {}
                    Ok(Some(dist_info)) => {
                        index.insert(dist_info.name().clone(), dist_info);
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
    pub fn get(&self, name: &PackageName) -> Option<&CachedRegistryDistribution> {
        self.0.get(name)
    }
}
