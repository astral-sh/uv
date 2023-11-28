use std::path::{Path, PathBuf};

use fxhash::FxHashMap;
use tracing::warn;
use url::Url;

use distribution_filename::WheelFilename;
use distribution_types::{CachedDirectUrlDist, Identifier};

use crate::cache::{CacheShard, WheelCache};

/// A local index of distributions that originate from arbitrary URLs (as opposed to being
/// downloaded from a registry, like `PyPI`).
#[derive(Debug, Default)]
pub(crate) struct UrlIndex(FxHashMap<String, PathBuf>);

impl UrlIndex {
    /// Build an index of cached distributions from a directory.
    pub(crate) fn try_from_directory(path: &Path) -> Self {
        let mut index = FxHashMap::default();

        let cache = WheelCache::new(path);
        let Ok(dir) = cache.read_dir(CacheShard::Url) else {
            return Self(index);
        };

        for entry in dir {
            let (file_type, entry) = match entry.and_then(|entry| Ok((entry.file_type()?, entry))) {
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
                let file_name = entry.file_name();
                let Some(filename) = file_name.to_str() else {
                    continue;
                };
                index.insert(filename.to_string(), entry.path());
            }
        }

        Self(index)
    }

    /// Returns a distribution from the index, if it exists.
    pub(crate) fn get(&self, filename: WheelFilename, url: &Url) -> Option<CachedDirectUrlDist> {
        // TODO(charlie): This takes advantage of the fact that for URL dependencies, the package ID
        // and distribution ID are identical. We should either change the cache layout to use
        // distribution IDs, or implement package ID for URL.
        let path = self.0.get(&url.distribution_id())?;
        Some(CachedDirectUrlDist::from_url(
            filename,
            url.clone(),
            path.clone(),
        ))
    }
}
