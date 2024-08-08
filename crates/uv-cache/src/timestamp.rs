use serde::{Deserialize, Serialize};
use std::path::Path;

/// A timestamp used to measure changes to a file.
///
/// On Unix, this uses `ctime` as a conservative approach. `ctime` should detect all
/// modifications, including some that we don't care about, like hardlink modifications.
/// On other platforms, it uses `mtime`.
///
/// See: <https://github.com/restic/restic/issues/2179>
/// See: <https://apenwarr.ca/log/20181113>
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
pub struct Timestamp(std::time::SystemTime);

impl Timestamp {
    /// Return the [`Timestamp`] for the given path.
    pub fn from_path(path: impl AsRef<Path>) -> std::io::Result<Self> {
        let metadata = fs_err::metadata(path.as_ref())?;
        Ok(Self::from_metadata(&metadata))
    }

    /// Return the [`Timestamp`] for the given metadata.
    pub fn from_metadata(metadata: &std::fs::Metadata) -> Self {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;

            let ctime = u64::try_from(metadata.ctime()).expect("ctime to be representable as u64");
            let ctime_nsec = u32::try_from(metadata.ctime_nsec())
                .expect("ctime_nsec to be representable as u32");
            let duration = std::time::Duration::new(ctime, ctime_nsec);
            Self(std::time::UNIX_EPOCH + duration)
        }

        #[cfg(not(unix))]
        {
            let modified = metadata.modified().expect("modified time to be available");
            Self(modified)
        }
    }

    /// Return the current [`Timestamp`].
    pub fn now() -> Self {
        Self(std::time::SystemTime::now())
    }
}
