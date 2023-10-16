use std::path::{Path, PathBuf};

use fs_err::tokio as fs;

static WHEEL_CACHE: &str = "wheels-v0";

#[derive(Debug)]
pub(crate) struct WheelCache {
    root: PathBuf,
}

impl WheelCache {
    /// Create a handle to the wheel cache.
    pub(crate) fn new(root: &Path) -> Self {
        Self {
            root: root.join(WHEEL_CACHE),
        }
    }

    /// Return the path at which a given wheel would be stored.
    pub(crate) fn entry(&self, id: &str) -> PathBuf {
        self.root.join(id)
    }

    /// Initialize the wheel cache.
    pub(crate) async fn init(&self) -> std::io::Result<()> {
        fs::create_dir_all(&self.root).await
    }

    /// Returns a handle to the wheel cache directory.
    pub(crate) async fn read_dir(&self) -> std::io::Result<fs::ReadDir> {
        fs::read_dir(&self.root).await
    }

    /// Returns the cache root.
    pub(crate) fn root(&self) -> &Path {
        &self.root
    }
}
