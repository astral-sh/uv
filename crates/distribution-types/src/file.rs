use std::fmt::{Display, Formatter};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;

use pep440_rs::{VersionSpecifiers, VersionSpecifiersParseError};
use pep508_rs::split_scheme;
use pypi_types::{DistInfoMetadata, HashDigest, Yanked};

/// Error converting [`pypi_types::File`] to [`distribution_type::File`].
#[derive(Debug, Error)]
pub enum FileConversionError {
    #[error("Failed to parse 'requires-python': `{0}`")]
    RequiresPython(String, #[source] VersionSpecifiersParseError),
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
    pub dist_info_metadata: bool,
    pub filename: String,
    pub hashes: Vec<HashDigest>,
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
    pub fn try_from(file: pypi_types::File, base: &Url) -> Result<Self, FileConversionError> {
        Ok(Self {
            dist_info_metadata: file
                .dist_info_metadata
                .as_ref()
                .is_some_and(DistInfoMetadata::is_available),
            filename: file.filename,
            hashes: file.hashes.into_digests(),
            requires_python: file
                .requires_python
                .transpose()
                .map_err(|err| FileConversionError::RequiresPython(err.line().clone(), err))?,
            size: file.size,
            upload_time_utc_ms: file.upload_time.map(|dt| dt.timestamp_millis()),
            url: {
                if split_scheme(&file.url).is_some() {
                    FileLocation::AbsoluteUrl(file.url)
                } else {
                    FileLocation::RelativeUrl(base.to_string(), file.url)
                }
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

impl FileLocation {
    /// Convert this location to a URL.
    ///
    /// A relative URL has its base joined to the path. An absolute URL is
    /// parsed as-is. And a path location is turned into a URL via the `file`
    /// protocol.
    ///
    /// # Errors
    ///
    /// This returns an error if any of the URL parsing fails, or if, for
    /// example, the location is a path and the path isn't valid UTF-8.
    /// (Because URLs must be valid UTF-8.)
    pub fn to_url(&self) -> Result<Url, ToUrlError> {
        match *self {
            FileLocation::RelativeUrl(ref base, ref path) => {
                let base_url = Url::parse(base).map_err(|err| ToUrlError::InvalidBase {
                    base: base.clone(),
                    err,
                })?;
                let joined = base_url.join(path).map_err(|err| ToUrlError::InvalidJoin {
                    base: base.clone(),
                    path: path.clone(),
                    err,
                })?;
                Ok(joined)
            }
            FileLocation::AbsoluteUrl(ref absolute) => {
                let url = Url::parse(absolute).map_err(|err| ToUrlError::InvalidAbsolute {
                    absolute: absolute.clone(),
                    err,
                })?;
                Ok(url)
            }
            FileLocation::Path(ref path) => {
                let path = path
                    .to_str()
                    .ok_or_else(|| ToUrlError::PathNotUtf8 { path: path.clone() })?;
                let url = Url::from_file_path(path).map_err(|()| ToUrlError::InvalidPath {
                    path: path.to_string(),
                })?;
                Ok(url)
            }
        }
    }
}

impl Display for FileLocation {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RelativeUrl(_base, url) => Display::fmt(&url, f),
            Self::AbsoluteUrl(url) => Display::fmt(&url, f),
            Self::Path(path) => Display::fmt(&path.display(), f),
        }
    }
}

/// An error that occurs when a `FileLocation` is not a valid URL.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ToUrlError {
    /// An error that occurs when the base URL in `FileLocation::Relative`
    /// could not be parsed as a valid URL.
    #[error("could not parse base URL `{base}` as a valid URL")]
    InvalidBase {
        /// The base URL that could not be parsed as a valid URL.
        base: String,
        /// The underlying URL parse error.
        #[source]
        err: url::ParseError,
    },
    /// An error that occurs when the base URL could not be joined with
    /// the relative path in a `FileLocation::Relative`.
    #[error("could not join base URL `{base}` to relative path `{path}`")]
    InvalidJoin {
        /// The base URL that could not be parsed as a valid URL.
        base: String,
        /// The relative path segment.
        path: String,
        /// The underlying URL parse error.
        #[source]
        err: url::ParseError,
    },
    /// An error that occurs when the absolute URL in `FileLocation::Absolute`
    /// could not be parsed as a valid URL.
    #[error("could not parse absolute URL `{absolute}` as a valid URL")]
    InvalidAbsolute {
        /// The absolute URL that could not be parsed as a valid URL.
        absolute: String,
        /// The underlying URL parse error.
        #[source]
        err: url::ParseError,
    },
    /// An error that occurs when the file path in `FileLocation::Path` is
    /// not valid UTF-8. We need paths to be valid UTF-8 to be transformed
    /// into URLs, which must also be UTF-8.
    #[error("could not build URL from file path `{path}` because it is not valid UTF-8")]
    PathNotUtf8 {
        /// The original path that was not valid UTF-8.
        path: PathBuf,
    },
    /// An error that occurs when the file URL created from a file path is not
    /// a valid URL.
    #[error("could not parse file path `{path}` as a valid URL")]
    InvalidPath {
        /// The file path URL that could not be parsed as a valid URL.
        path: String,
    },
}
