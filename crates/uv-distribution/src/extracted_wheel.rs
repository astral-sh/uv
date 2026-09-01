use std::fmt::Display;
use std::path::Path;

use either::Either;
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

/// Files extracted from a wheel, with or without content-addressing metadata.
pub(crate) enum ExtractedWheel {
    Unhashed(Vec<UnhashedFile>),
    Hashed(HashedWheel),
}

impl ExtractedWheel {
    /// Extract a wheel from a streaming reader, optionally retaining its per-file digests.
    ///
    /// See [`uv_extract::stream::unzip`] for buffering, cleanup, and download hash requirements.
    pub(crate) async fn extract_streaming<R>(
        reader: R,
        temp_dir: tempfile::TempDir,
        content_addressed: bool,
    ) -> Result<(tempfile::TempDir, Self), uv_extract::Error>
    where
        R: AsyncRead + Unpin,
    {
        if content_addressed {
            let (temp_dir, files, tree) =
                uv_extract::stream::unzip_and_hash(reader, temp_dir).await?;
            Ok((temp_dir, Self::Hashed(HashedWheel { files, tree })))
        } else {
            let (temp_dir, files) = uv_extract::stream::unzip(reader, temp_dir).await?;
            Ok((temp_dir, Self::Unhashed(files)))
        }
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
