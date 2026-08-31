//! Filesystem operations shared by asynchronous and buffered ZIP extraction.

use std::io;
use std::path::Path;

use futures::io::AllowStdIo;
use tokio::io::AsyncWrite;
use tokio_util::compat::FuturesAsyncWriteCompatExt;
use tokio_util::either::Either;

/// Select synchronous operations only when the entire extraction runs on a blocking thread.
pub(super) struct Filesystem<const BLOCKING: bool>;

impl<const BLOCKING: bool> Filesystem<BLOCKING> {
    pub(super) async fn create_dir_all(path: &Path) -> io::Result<()> {
        if BLOCKING {
            fs_err::create_dir_all(path)
        } else {
            fs_err::tokio::create_dir_all(path).await
        }
    }

    pub(super) async fn create_file(path: &Path) -> io::Result<impl AsyncWrite + Unpin> {
        if BLOCKING {
            let file = fs_err::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)?;
            Ok(Either::Left(AllowStdIo::new(file).compat_write()))
        } else {
            let file = fs_err::tokio::File::create_new(path).await?;
            Ok(Either::Right(file))
        }
    }

    pub(super) async fn read(path: &Path) -> io::Result<Vec<u8>> {
        if BLOCKING {
            fs_err::read(path)
        } else {
            fs_err::tokio::read(path).await
        }
    }

    #[cfg(unix)]
    pub(super) async fn metadata(path: &Path) -> io::Result<std::fs::Metadata> {
        if BLOCKING {
            fs_err::metadata(path)
        } else {
            fs_err::tokio::metadata(path).await
        }
    }

    #[cfg(unix)]
    pub(super) async fn set_permissions(
        path: &Path,
        permissions: std::fs::Permissions,
    ) -> io::Result<()> {
        if BLOCKING {
            fs_err::set_permissions(path, permissions)
        } else {
            fs_err::tokio::set_permissions(path, permissions).await
        }
    }
}
