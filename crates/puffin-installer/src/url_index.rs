use std::path::{Path, PathBuf};

use anyhow::Result;
use fxhash::FxHashMap;
use url::Url;

use crate::cache::{CacheShard, WheelCache};
use puffin_distribution::{CachedDistribution, RemoteDistributionRef};
use puffin_package::package_name::PackageName;

/// A local index of distributions that originate from arbitrary URLs (as opposed to being
/// downloaded from a registry, like `PyPI`).
#[derive(Debug, Default)]
pub(crate) struct UrlIndex(FxHashMap<String, PathBuf>);

impl UrlIndex {
    /// Build an index of cached distributions from a directory.
    pub(crate) fn try_from_directory(path: &Path) -> Result<Self> {
        let mut index = FxHashMap::default();

        let cache = WheelCache::new(path);
        let Ok(dir) = cache.read_dir(CacheShard::Url) else {
            return Ok(Self(index));
        };

        for entry in dir {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                let file_name = entry.file_name();
                let Some(filename) = file_name.to_str() else {
                    continue;
                };
                index.insert(filename.to_string(), entry.path());
            }
        }

        Ok(Self(index))
    }

    /// Returns a distribution from the index, if it exists.
    pub(crate) fn get(&self, name: &PackageName, url: &Url) -> Option<CachedDistribution> {
        let distribution = RemoteDistributionRef::from_url(name, url);
        let path = self.0.get(&distribution.id())?;
        Some(CachedDistribution::Url(
            name.clone(),
            url.clone(),
            path.clone(),
        ))
    }
}
