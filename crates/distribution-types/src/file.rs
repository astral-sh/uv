use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;

use pep440_rs::{VersionSpecifiers, VersionSpecifiersParseError};
use pypi_types::{BaseUrl, DistInfoMetadata, Hashes, Yanked};

/// Error converting [`pypi_types::File`] to [`distribution_type::File`].
#[derive(Debug, Error)]
pub enum FileConversionError {
    #[error("Invalid requires python version specifier")]
    VersionSpecifiersParseError(#[from] VersionSpecifiersParseError),
    #[error("Failed to parse URL: {0}")]
    Url(String, #[source] url::ParseError),
}

/// Internal analog to [`pypi_types::File`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct File {
    pub dist_info_metadata: Option<DistInfoMetadata>,
    pub filename: String,
    pub hashes: Hashes,
    pub requires_python: Option<VersionSpecifiers>,
    pub size: Option<u64>,
    pub upload_time: Option<DateTime<Utc>>,
    pub url: Url,
    pub yanked: Option<Yanked>,
}

impl File {
    /// `TryFrom` instead of `From` to filter out files with invalid requires python version specifiers
    pub fn try_from(file: pypi_types::File, base: &BaseUrl) -> Result<Self, FileConversionError> {
        Ok(Self {
            dist_info_metadata: file.dist_info_metadata,
            filename: file.filename,
            hashes: file.hashes,
            requires_python: file.requires_python.transpose()?,
            size: file.size,
            upload_time: file.upload_time,
            url: base
                .join_relative(&file.url)
                .map_err(|err| FileConversionError::Url(file.url.clone(), err))?,
            yanked: file.yanked,
        })
    }
}
