use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;

use puffin_package::package_name::PackageName;

use crate::cache::WheelCache;
use crate::distribution::LocalDistribution;

/// A local index of cached distributions.
#[derive(Debug, Default)]
pub struct LocalIndex(HashMap<PackageName, LocalDistribution>);

impl LocalIndex {
    /// Build an index of cached distributions from a directory.
    pub async fn from_directory(path: &Path) -> Result<Self> {
        let mut index = HashMap::new();

        let cache = WheelCache::new(path);
        let Ok(mut dir) = cache.read_dir().await else {
            return Ok(Self(index));
        };

        while let Some(entry) = dir.next_entry().await? {
            if entry.file_type().await?.is_dir() {
                if let Some(dist_info) = LocalDistribution::try_from_path(&entry.path())? {
                    index.insert(dist_info.name().clone(), dist_info);
                }
            }
        }

        Ok(Self(index))
    }

    /// Returns a distribution from the index, if it exists.
    pub fn get(&self, name: &PackageName) -> Option<&LocalDistribution> {
        self.0.get(name)
    }
}
