use fs_err as fs;
use tracing::warn;

use distribution_types::CachedWheel;
use platform_tags::Tags;
use puffin_cache::CacheShard;

use crate::index::iter_directories;

/// A local index of built distributions for a specific source distribution.
pub struct BuiltWheelIndex;

impl BuiltWheelIndex {
    /// Find the "best" distribution in the index for a given source distribution.
    ///
    /// This lookup prefers newer versions over older versions, and aims to maximize compatibility
    /// with the target platform.
    ///
    /// The `shard` should point to a directory containing the built distributions for a specific
    /// source distribution. For example, given the built wheel cache structure:
    /// ```text
    /// built-wheels-v0/
    /// └── pypi
    ///     └── django-allauth-0.51.0.tar.gz
    ///         ├── django_allauth-0.51.0-py3-none-any.whl
    ///         └── metadata.json
    /// ```
    ///
    /// The `shard` should be `built-wheels-v0/pypi/django-allauth-0.51.0.tar.gz`.
    pub fn find(shard: &CacheShard, tags: &Tags) -> Option<CachedWheel> {
        let mut candidate: Option<CachedWheel> = None;

        for subdir in iter_directories(shard.read_dir().ok()?) {
            match CachedWheel::from_path(&subdir) {
                Ok(None) => {}
                Ok(Some(dist_info)) => {
                    // Pick the wheel with the highest priority
                    let compatibility = dist_info.filename.compatibility(tags);

                    // Only consider wheels that are compatible with our tags.
                    if compatibility.is_none() {
                        continue;
                    }

                    // TODO(charlie): Consider taking into account the freshness checks that we
                    // encode when building source distributions (e.g., timestamps). For now, we
                    // assume that distributions are immutable when installing (i.e., in this
                    // index).
                    if let Some(existing) = candidate.as_ref() {
                        // Override if the wheel is newer, or "more" compatible.
                        if dist_info.filename.version > existing.filename.version
                            || compatibility > existing.filename.compatibility(tags)
                        {
                            candidate = Some(dist_info);
                        }
                    } else {
                        candidate = Some(dist_info);
                    }
                }
                Err(err) => {
                    warn!(
                        "Invalid cache entry at {}, removing. {err}",
                        subdir.display()
                    );
                    if let Err(err) = fs::remove_dir_all(&subdir) {
                        warn!(
                            "Failed to remove invalid cache entry at {}: {err}",
                            subdir.display()
                        );
                    }
                }
            }
        }

        candidate
    }
}
