use std::io;
use std::path::PathBuf;
use std::sync::Arc;

use fs2::FileExt;
use fs_err::File;
use rustc_hash::FxHashMap;
use tokio::sync::Mutex;
use tracing::error;

use distribution_types::{Identifier, ResourceId};

/// A set of locks used to prevent concurrent access to the same resource.
#[derive(Debug, Default)]
pub(crate) struct Locks(Mutex<FxHashMap<ResourceId, Arc<Mutex<()>>>>);

impl Locks {
    /// Acquire a lock on the given resource.
    pub(crate) async fn acquire(&self, dist: &impl Identifier) -> Arc<Mutex<()>> {
        let mut map = self.0.lock().await;
        map.entry(dist.resource_id())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }
}

/// A file lock that is automatically released when dropped.
#[derive(Debug)]
pub(crate) struct LockedFile(File);

impl LockedFile {
    pub(crate) fn new(path: impl Into<PathBuf>) -> Result<Self, io::Error> {
        let file = File::create(path)?;
        // TODO(konstin): Notify the user when the lock isn't free so they know why nothing is
        // happening
        file.file().lock_exclusive()?;
        Ok(Self(file))
    }
}

impl Drop for LockedFile {
    fn drop(&mut self) {
        if let Err(err) = self.0.file().unlock() {
            error!(
                "Failed to unlock {}; program may be stuck: {}",
                self.0.path().display(),
                err
            );
        }
    }
}
