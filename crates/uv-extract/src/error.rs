use std::{ffi::OsString, path::PathBuf};

use zip::result::ZipError;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Zip(#[from] ZipError),
    #[error(transparent)]
    AsyncZip(#[from] async_zip::error::ZipError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("Unsupported archive type: {0}")]
    UnsupportedArchive(PathBuf),
    #[error(
        "The top level of the archive must only contain a list directory, but it contains: {0:?}"
    )]
    InvalidArchive(Vec<OsString>),
}
