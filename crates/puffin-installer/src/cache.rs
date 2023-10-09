use std::path::{Path, PathBuf};

static WHEEL_CACHE: &str = "wheels-v0";

#[derive(Debug)]
pub(crate) struct WheelCache<'a> {
    path: &'a Path,
}

impl<'a> WheelCache<'a> {
    /// Create a handle to the wheel cache.
    pub(crate) fn new(path: &'a Path) -> Self {
        Self { path }
    }

    /// Return the path at which a given wheel would be stored.
    pub(crate) fn entry(&self, id: &str) -> PathBuf {
        self.path.join(WHEEL_CACHE).join(id)
    }

    /// Initialize the wheel cache.
    pub(crate) async fn init(&self) -> std::io::Result<()> {
        tokio::fs::create_dir_all(self.path.join(WHEEL_CACHE)).await
    }

    /// Returns a handle to the wheel cache directory.
    pub(crate) async fn read_dir(&self) -> std::io::Result<tokio::fs::ReadDir> {
        tokio::fs::read_dir(self.path.join(WHEEL_CACHE)).await
    }
}
