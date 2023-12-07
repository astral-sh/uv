use std::path::PathBuf;

use fs_err as fs;
use tracing::warn;

use distribution_types::CachedWheel;
use platform_tags::Tags;

use crate::index::iter_directories;

/// A local index of built distributions for a specific source distribution.
#[derive(Debug)]
pub struct BuiltWheelIndex<'a> {
    directory: PathBuf,
    tags: &'a Tags,
}

impl<'a> BuiltWheelIndex<'a> {
    /// Create a new index of built distributions.
    ///
    /// The `directory` should be the directory containing the built distributions for a specific
    /// source distribution. For example, given the built wheel cache structure:
    /// ```text
    /// built-wheels-v0/
    /// └── pypi
    ///     └── django-allauth-0.51.0.tar.gz
    ///         ├── django_allauth-0.51.0-py3-none-any.whl
    ///         └── metadata.json
    /// ```
    ///
    /// The `directory` should be `built-wheels-v0/pypi/django-allauth-0.51.0.tar.gz`.
    pub fn new(directory: impl Into<PathBuf>, tags: &'a Tags) -> Self {
        Self {
            directory: directory.into(),
            tags,
        }
    }

    /// Find the "best" distribution in the index.
    ///
    /// This lookup prefers newer versions over older versions, and aims to maximize compatibility
    /// with the target platform.
    pub fn find(&self) -> Option<CachedWheel> {
        let mut candidate: Option<CachedWheel> = None;

        for subdir in iter_directories(self.directory.read_dir().ok()?) {
            match CachedWheel::from_path(&subdir) {
                Ok(None) => {}
                Ok(Some(dist_info)) => {
                    // Pick the wheel with the highest priority
                    let compatibility = dist_info.filename.compatibility(self.tags);

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
                            || compatibility > existing.filename.compatibility(self.tags)
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
                    let result = fs::remove_dir_all(&subdir);
                    if let Err(err) = result {
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
