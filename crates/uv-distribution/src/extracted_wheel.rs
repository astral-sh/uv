use std::fmt::Display;
use std::io;
use std::path::Path;

use either::Either;
use tempfile::TempDir;
use tokio::io::AsyncRead;

use uv_extract::dirhash::{DirhashTree, HashedFile, UnhashedFile, dirhash_path};
use uv_fs::PortablePath;
use uv_install_wheel::validate_and_heal_record;

use crate::Error;

/// Per-file digests and the hash tree of an extracted wheel.
pub(crate) struct HashedWheel {
    pub(crate) files: Vec<HashedFile>,
    pub(crate) tree: DirhashTree,
}

/// A temporary directory and configuration for extracting a wheel.
pub(crate) struct WheelExtractor {
    temp_dir: TempDir,
    content_addressed: bool,
}

/// An extracted wheel that owns its temporary directory until persistence.
pub(crate) struct ExtractedWheel {
    temp_dir: TempDir,
    files: ExtractedFiles,
}

/// Files extracted from a wheel, with or without content-addressing metadata.
enum ExtractedFiles {
    Unhashed(Vec<UnhashedFile>),
    Hashed(HashedWheel),
}

impl WheelExtractor {
    /// Create a temporary directory under the cache root for extracting a wheel.
    pub(crate) fn new(root: &Path, content_addressed: bool) -> io::Result<Self> {
        Ok(Self {
            temp_dir: tempfile::tempdir_in(root)?,
            content_addressed,
        })
    }

    /// Extract a wheel from a streaming reader, optionally retaining its per-file digests.
    ///
    /// See [`uv_extract::stream::unzip`] for buffering, cleanup, and download hash requirements.
    pub(crate) async fn extract_streaming<R>(
        self,
        reader: R,
    ) -> Result<ExtractedWheel, uv_extract::Error>
    where
        R: AsyncRead + Unpin,
    {
        if self.content_addressed {
            let (temp_dir, files, tree) =
                uv_extract::stream::unzip_and_hash(reader, self.temp_dir).await?;
            Ok(ExtractedWheel {
                temp_dir,
                files: ExtractedFiles::Hashed(HashedWheel { files, tree }),
            })
        } else {
            let (temp_dir, files) = uv_extract::stream::unzip(reader, self.temp_dir).await?;
            Ok(ExtractedWheel {
                temp_dir,
                files: ExtractedFiles::Unhashed(files),
            })
        }
    }

    /// Extract a wheel from a seekable file, optionally retaining its per-file digests.
    pub(crate) fn extract_seekable(
        self,
        reader: fs_err::File,
    ) -> Result<ExtractedWheel, uv_extract::Error> {
        let files = if self.content_addressed {
            let (files, tree) = uv_extract::unzip_and_hash(reader, self.temp_dir.path())?;
            ExtractedFiles::Hashed(HashedWheel { files, tree })
        } else {
            let files = uv_extract::unzip(reader, self.temp_dir.path())?;
            ExtractedFiles::Unhashed(files)
        };
        Ok(ExtractedWheel {
            temp_dir: self.temp_dir,
            files,
        })
    }
}

impl ExtractedWheel {
    /// Return the temporary directory and optional content hashes for persistence.
    pub(crate) fn into_parts(self) -> (TempDir, Option<HashedWheel>) {
        let hashed_wheel = match self.files {
            ExtractedFiles::Unhashed(_) => None,
            ExtractedFiles::Hashed(wheel) => Some(wheel),
        };
        (self.temp_dir, hashed_wheel)
    }

    /// Heal the wheel's `RECORD` and keep its hash tree consistent with the repaired contents.
    pub(crate) fn validate_and_heal_record(&mut self, dist: impl Display) -> Result<(), Error> {
        let root = self.temp_dir.path();
        let files = match &self.files {
            ExtractedFiles::Unhashed(files) => {
                Either::Left(files.iter().map(|file| (file.path(), file.size())))
            }
            ExtractedFiles::Hashed(wheel) => {
                Either::Right(wheel.files.iter().map(|file| (file.path(), file.size())))
            }
        };
        let Some(record_path) =
            validate_and_heal_record(root, files, dist).map_err(Error::InstallWheelError)?
        else {
            return Ok(());
        };
        let ExtractedFiles::Hashed(hashed_wheel) = &mut self.files else {
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
