use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;

use puffin_distribution::CachedDistribution;
use puffin_normalize::PackageName;

use crate::cache::{CacheShard, WheelCache};

/// A local index of distributions that originate from a registry, like `PyPI`.
#[derive(Debug, Default)]
pub struct RegistryIndex(HashMap<PackageName, CachedDistribution>);

impl RegistryIndex {
    /// Build an index of cached distributions from a directory.
    pub fn try_from_directory(path: &Path) -> Result<Self> {
        let mut index = HashMap::new();

        let cache = WheelCache::new(path);
        let Ok(dir) = cache.read_dir(CacheShard::Registry) else {
            return Ok(Self(index));
        };

        for entry in dir {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                if let Some(dist_info) = CachedDistribution::try_from_path(&entry.path())? {
                    index.insert(dist_info.name().clone(), dist_info);
                }
            }
        }

        Ok(Self(index))
    }

    /// Returns a distribution from the index, if it exists.
    pub fn get(&self, name: &PackageName) -> Option<&CachedDistribution> {
        self.0.get(name)
    }
}
