use std::fmt::Display;
use std::io;
use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};

use either::Either;
use futures::future::{AbortHandle, Aborted};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};

use uv_extract::dirhash::{DirhashTree, HashedFile, UnhashedFile, dirhash_path};
use uv_extract::stream::DEFAULT_BUF_SIZE;
use uv_fs::PortablePath;
use uv_install_wheel::validate_and_heal_record;

use crate::Error;

/// Per-file digests and the hash tree of an extracted wheel.
pub(crate) struct HashedWheel {
    pub(crate) files: Vec<HashedFile>,
    pub(crate) tree: DirhashTree,
}

/// Files extracted from a wheel, with or without content-addressing metadata.
pub(crate) enum ExtractedWheel {
    Unhashed(Vec<UnhashedFile>),
    Hashed(HashedWheel),
}

/// Stop the extraction worker before removing its temporary directory.
///
/// The worker holds the directory's lock throughout extraction. Dropping the guard cancels active
/// work and waits for that lock, or takes the directory immediately if the worker is still queued.
/// Cleanup completes without relying on the async runtime.
struct ExtractionGuard {
    abort: AbortHandle,
    temp_dir: Arc<Mutex<Option<tempfile::TempDir>>>,
}

impl Drop for ExtractionGuard {
    fn drop(&mut self) {
        self.abort.abort();
        self.temp_dir
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
    }
}

impl ExtractedWheel {
    /// Feed an archive reader through a bounded pipe to a single extraction worker.
    ///
    /// Filesystem operations run synchronously on the worker, while reading and hashing the archive
    /// remain asynchronous. The pipe applies backpressure without buffering the entire wheel.
    /// Dropping the future cancels extraction and removes its temporary directory synchronously.
    /// Successful extraction returns ownership of the directory to the caller.
    ///
    /// Extraction can leave unread bytes when ZIP validation is disabled. Callers must drain the
    /// reader before finalizing download hashes.
    pub(crate) async fn extract_streaming<R>(
        mut reader: R,
        temp_dir: tempfile::TempDir,
        content_addressed: bool,
    ) -> Result<(tempfile::TempDir, Self), uv_extract::Error>
    where
        R: AsyncRead + Unpin,
    {
        const PIPE_BUFFER_SIZE: usize = 2 * DEFAULT_BUF_SIZE;

        // Allow the download to get ahead while the worker decompresses and writes files.
        let (sender, receiver) = tokio::io::duplex(PIPE_BUFFER_SIZE);
        let (abort, registration) = AbortHandle::new_pair();
        let guard = ExtractionGuard {
            abort,
            temp_dir: Arc::new(Mutex::new(Some(temp_dir))),
        };
        let temp_dir = Arc::clone(&guard.temp_dir);
        let mut extraction = tokio::task::spawn_blocking(move || {
            let temp_dir = temp_dir.lock().unwrap_or_else(PoisonError::into_inner);
            let Some(temp_dir) = temp_dir.as_ref() else {
                return Err(uv_extract::Error::Io(io::Error::other(Aborted)));
            };
            let extracted = if content_addressed {
                let (files, tree) = uv_extract::stream::unzip_blocking_and_hash(
                    receiver,
                    temp_dir.path(),
                    registration,
                )?;
                Self::Hashed(HashedWheel { files, tree })
            } else {
                let files =
                    uv_extract::stream::unzip_blocking(receiver, temp_dir.path(), registration)?;
                Self::Unhashed(files)
            };
            Ok::<_, uv_extract::Error>(extracted)
        });
        let download = async {
            // Own the write end so EOF, errors and cancellation all close the pipe.
            let mut sender = sender;
            let mut buffer = vec![0; DEFAULT_BUF_SIZE];
            loop {
                let read = reader.read(&mut buffer).await?;
                if read == 0 {
                    break;
                }
                if let Err(err) = sender.write_all(&buffer[..read]).await {
                    if err.kind() == io::ErrorKind::BrokenPipe {
                        // The worker either rejected the archive or finished early because ZIP
                        // validation is disabled. The caller drains the download in the latter case.
                        break;
                    }
                    return Err(err);
                }
            }
            Ok::<_, io::Error>(())
        };
        let extraction = tokio::select! {
            // Prefer a download error over the resulting truncated-ZIP error if both are ready.
            biased;
            download = download => {
                if download.is_err() {
                    guard.abort.abort();
                }
                // Await the worker so the guard does not block this runtime thread on cleanup.
                let extraction = extraction.await;
                download.map_err(uv_extract::Error::Io)?;
                extraction
            }
            // Stop reading even if the server stalls after sending an invalid ZIP entry.
            extraction = &mut extraction => extraction,
        };
        let extracted =
            extraction.map_err(|err| uv_extract::Error::Io(io::Error::other(err)))??;
        let temp_dir = guard
            .temp_dir
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
            .ok_or_else(|| uv_extract::Error::Io(io::Error::other(Aborted)))?;
        Ok((temp_dir, extracted))
    }

    /// Extract a wheel from a seekable file, optionally retaining its per-file digests.
    pub(crate) fn extract_seekable(
        reader: fs_err::File,
        target: &Path,
        content_addressed: bool,
    ) -> Result<Self, uv_extract::Error> {
        if content_addressed {
            let (files, tree) = uv_extract::unzip_and_hash(reader, target)?;
            Ok(Self::Hashed(HashedWheel { files, tree }))
        } else {
            let files = uv_extract::unzip(reader, target)?;
            Ok(Self::Unhashed(files))
        }
    }

    /// Return the hashed wheel if content hashing was enabled.
    pub(crate) fn into_hashed(self) -> Option<HashedWheel> {
        match self {
            Self::Unhashed(_) => None,
            Self::Hashed(wheel) => Some(wheel),
        }
    }

    /// Heal the wheel's `RECORD` and keep its hash tree consistent with the repaired contents.
    pub(crate) fn validate_and_heal_record(
        &mut self,
        root: &Path,
        dist: impl Display,
    ) -> Result<(), Error> {
        let files = match self {
            Self::Unhashed(files) => {
                Either::Left(files.iter().map(|file| (file.path(), file.size())))
            }
            Self::Hashed(wheel) => {
                Either::Right(wheel.files.iter().map(|file| (file.path(), file.size())))
            }
        };
        let Some(record_path) =
            validate_and_heal_record(root, files, dist).map_err(Error::InstallWheelError)?
        else {
            return Ok(());
        };
        let Self::Hashed(hashed_wheel) = self else {
            return Ok(());
        };

        let hash = dirhash_path(&root.join(&record_path)).map_err(|err| {
            Error::Extract(
                record_path.display().to_string(),
                uv_extract::Error::from(err),
            )
        })?;
        let record_path = PortablePath::from(record_path.as_path()).to_string();
        hashed_wheel
            .tree
            .update_file(&record_path, hash)
            .map_err(|err| Error::Extract(record_path, uv_extract::Error::from(err)))
    }
}
