use std::fmt::{Display, Formatter};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use pep440_rs::{VersionSpecifiers, VersionSpecifiersParseError};
use pypi_types::{DistInfoMetadata, Hashes, Yanked};

/// Error converting [`pypi_types::File`] to [`distribution_type::File`].
#[derive(Debug, Error)]
pub enum FileConversionError {
    #[error("Invalid 'requires-python' value")]
    VersionSpecifiersParseError(#[from] VersionSpecifiersParseError),
    #[error("Failed to parse URL: {0}")]
    Url(String, #[source] url::ParseError),
}

/// Internal analog to [`pypi_types::File`].
#[derive(
    Debug, Clone, Serialize, Deserialize, rkyv::Archive, rkyv::Deserialize, rkyv::Serialize,
)]
#[archive(check_bytes)]
#[archive_attr(derive(Debug))]
pub struct File {
    pub dist_info_metadata: Option<DistInfoMetadata>,
    pub filename: String,
    pub hashes: Hashes,
    pub requires_python: Option<VersionSpecifiers>,
    pub size: Option<u64>,
    // N.B. We don't use a chrono DateTime<Utc> here because it's a little
    // annoying to do so with rkyv. Since we only use this field for doing
    // comparisons in testing, we just store it as a UTC timestamp in
    // milliseconds.
    pub upload_time_utc_ms: Option<i64>,
    pub url: FileLocation,
    pub yanked: Option<Yanked>,
}

impl File {
    /// `TryFrom` instead of `From` to filter out files with invalid requires python version specifiers
    pub fn try_from(file: pypi_types::File, base: &str) -> Result<Self, FileConversionError> {
        Ok(Self {
            dist_info_metadata: file.dist_info_metadata,
            filename: file.filename,
            hashes: file.hashes,
            requires_python: file.requires_python.transpose()?,
            size: file.size,
            upload_time_utc_ms: file.upload_time.map(|dt| dt.timestamp_millis()),
            url: if file.url.contains("://") {
                FileLocation::AbsoluteUrl(file.url)
            } else {
                FileLocation::RelativeUrl(base.to_string(), file.url)
            },
            yanked: file.yanked,
        })
    }
}

/// While a registry file is generally a remote URL, it can also be a file if it comes from a directory flat indexes.
#[derive(
    Debug, Clone, Serialize, Deserialize, rkyv::Archive, rkyv::Deserialize, rkyv::Serialize,
)]
#[archive(check_bytes)]
#[archive_attr(derive(Debug))]
pub enum FileLocation {
    /// URL relative to the base URL.
    RelativeUrl(String, String),
    /// Absolute URL.
    AbsoluteUrl(String),
    /// Absolute path to a file.
    Path(#[with(rkyv::with::AsString)] PathBuf),
}

impl Display for FileLocation {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            FileLocation::RelativeUrl(_base, url) => Display::fmt(&url, f),
            FileLocation::AbsoluteUrl(url) => Display::fmt(&url, f),
            FileLocation::Path(path) => Display::fmt(&path.display(), f),
        }
    }
}
