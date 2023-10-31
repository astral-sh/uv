use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;

use puffin_distribution::CachedDistribution;
use puffin_package::package_name::PackageName;

use crate::cache::WheelCache;

/// A local index of cached distributions.
#[derive(Debug, Default)]
pub struct LocalIndex(HashMap<PackageName, CachedDistribution>);

impl LocalIndex {
    /// Build an index of cached distributions from a directory.
    pub fn try_from_directory(path: &Path) -> Result<Self> {
        let mut index = HashMap::new();

        let cache = WheelCache::new(path);
        let Ok(dir) = cache.read_dir() else {
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
