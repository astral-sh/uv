use std::ffi::{OsStr, OsString};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use fs_err::OpenOptions;
use rustc_hash::FxHashSet;
use tracing::debug;
use uv_fs::LockedFile;

use crate::CacheBucket;

/// The interval used to coalesce repeated last-use updates.
pub(crate) const UPDATE_INTERVAL: Duration = Duration::from_hours(24);

/// Deferred archive use, flushed while its shared cache lock is still held.
#[derive(Debug)]
pub(crate) struct CacheUsage {
    root: PathBuf,
    archive_root: PathBuf,
    canonical_archive_root: PathBuf,
    archives: Mutex<FxHashSet<OsString>>,
    // Retain the lock through `Drop`, even if the owning `Cache` releases its guard first.
    _lock_file: Arc<LockedFile>,
}

impl CacheUsage {
    pub(crate) fn new(root: PathBuf, lock_file: Arc<LockedFile>) -> Self {
        let canonical_root = fs_err::canonicalize(&root).unwrap_or_else(|_| root.clone());
        Self {
            archive_root: root.join(CacheBucket::Archive.to_str()),
            canonical_archive_root: canonical_root.join(CacheBucket::Archive.to_str()),
            root,
            archives: Mutex::default(),
            _lock_file: lock_file,
        }
    }

    /// Queue the use of an archive, accepting both lexical and canonical cache paths.
    pub(crate) fn record(&self, path: &Path) {
        if path.parent().is_some_and(|parent| {
            parent == self.archive_root || parent == self.canonical_archive_root
        }) && let Some(name) = path.file_name()
            && let Ok(mut archives) = self.archives.lock()
        {
            archives.insert(name.to_os_string());
        }
    }
}

impl Drop for CacheUsage {
    fn drop(&mut self) {
        let Ok(archives) = self.archives.get_mut() else {
            debug!("Skipping cache usage updates after a poisoned lock");
            return;
        };
        for archive in archives.iter() {
            if let Err(err) = touch(&self.root, archive) {
                debug!("Failed to record cache archive use for {archive:?}: {err}");
            }
        }
    }
}

/// Return the usage marker for an archive directly inside the archive bucket.
pub(crate) fn marker_path(root: &Path, archive: &OsStr) -> PathBuf {
    root.join(CacheBucket::Usage.to_str())
        .join(CacheBucket::Archive.to_str())
        .join(archive)
}

/// Read an explicit last-use timestamp without following a marker symlink.
pub(crate) fn last_used(marker: &Path) -> io::Result<Option<SystemTime>> {
    match fs_err::symlink_metadata(marker) {
        Ok(metadata) if metadata.is_file() => metadata.modified().map(Some),
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "Cache usage marker is not a regular file: {}",
                marker.display()
            ),
        )),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err),
    }
}

/// Record use at flush time, limiting writes to approximately one per archive per day.
pub(crate) fn touch(root: &Path, archive: &OsStr) -> io::Result<()> {
    let marker = marker_path(root, archive);
    if let Some(last_used) = last_used(&marker)?
        && !SystemTime::now()
            .duration_since(last_used)
            .is_ok_and(|elapsed| elapsed >= UPDATE_INTERVAL)
    {
        // Future timestamps are preserved if the system clock moves backwards.
        return Ok(());
    }

    let usage_root = root.join(CacheBucket::Usage.to_str());
    ensure_directory(&usage_root)?;
    ensure_directory(&usage_root.join(CacheBucket::Archive.to_str()))?;

    let file = match OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&marker)
    {
        Ok(file) => file,
        Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {
            // Another reader may have created or refreshed this marker while we were flushing.
            if let Some(last_used) = last_used(&marker)?
                && !SystemTime::now()
                    .duration_since(last_used)
                    .is_ok_and(|elapsed| elapsed >= UPDATE_INTERVAL)
            {
                return Ok(());
            }
            OpenOptions::new().write(true).open(&marker)?
        }
        Err(err) => return Err(err),
    };
    file.set_modified(SystemTime::now())
}

/// Create a marker directory, rejecting existing links or other non-directory entries.
fn ensure_directory(path: &Path) -> io::Result<()> {
    match fs_err::create_dir(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {
            if fs_err::symlink_metadata(path)?.is_dir() {
                Ok(())
            } else {
                Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Cache usage path is not a directory: {}", path.display()),
                ))
            }
        }
        Err(err) => Err(err),
    }
}
