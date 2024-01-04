use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use pep440_rs::{VersionSpecifiers, VersionSpecifiersParseError};
use pypi_types::{DistInfoMetadata, Hashes, Yanked};

/// Internal analog to [`pypi_types::File`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct File {
    pub dist_info_metadata: Option<DistInfoMetadata>,
    pub filename: String,
    pub hashes: Hashes,
    pub requires_python: Option<VersionSpecifiers>,
    pub size: Option<usize>,
    pub upload_time: Option<DateTime<Utc>>,
    pub url: String,
    pub yanked: Option<Yanked>,
}

impl TryFrom<pypi_types::File> for File {
    type Error = VersionSpecifiersParseError;

    /// `TryFrom` instead of `From` to filter out files with invalid requires python version specifiers
    fn try_from(file: pypi_types::File) -> Result<Self, Self::Error> {
        Ok(Self {
            dist_info_metadata: file.dist_info_metadata,
            filename: file.filename,
            hashes: file.hashes,
            requires_python: file.requires_python.transpose()?,
            size: file.size,
            upload_time: file.upload_time,
            url: file.url,
            yanked: file.yanked,
        })
    }
}
