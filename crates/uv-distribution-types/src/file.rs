use std::fmt::{self, Display, Formatter};
use std::str::FromStr;

use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use url::Url;

use uv_pep440::{VersionSpecifiers, VersionSpecifiersParseError};
use uv_pep508::split_scheme;
use uv_pypi_types::{CoreMetadata, HashDigests, Yanked};
use uv_small_str::SmallString;

/// Error converting [`uv_pypi_types::File`] to [`distribution_type::File`].
#[derive(Debug, thiserror::Error)]
pub enum FileConversionError {
    #[error("Failed to parse `requires-python`: `{0}`")]
    RequiresPython(String, #[source] VersionSpecifiersParseError),
    #[error("Failed to parse URL: {0}")]
    Url(String, #[source] url::ParseError),
}

/// Internal analog to [`uv_pypi_types::File`].
#[derive(Debug, Clone, PartialEq, Eq, Hash, rkyv::Archive, rkyv::Deserialize, rkyv::Serialize)]
#[rkyv(derive(Debug))]
pub struct File {
    pub dist_info_metadata: bool,
    pub filename: SmallString,
    pub hashes: HashDigests,
    pub requires_python: Option<VersionSpecifiers>,
    pub size: Option<u64>,
    // N.B. We don't use a Jiff timestamp here because it's a little
    // annoying to do so with rkyv. Since we only use this field for doing
    // comparisons in testing, we just store it as a UTC timestamp in
    // milliseconds.
    pub upload_time_utc_ms: Option<i64>,
    pub url: FileLocation,
    pub yanked: Option<Box<Yanked>>,
}

impl File {
    /// `TryFrom` instead of `From` to filter out files with invalid requires python version specifiers
    pub fn try_from(
        file: uv_pypi_types::File,
        base: &SmallString,
    ) -> Result<Self, FileConversionError> {
        Ok(Self {
            dist_info_metadata: file
                .core_metadata
                .as_ref()
                .is_some_and(CoreMetadata::is_available),
            filename: file.filename,
            hashes: HashDigests::from(file.hashes),
            requires_python: file
                .requires_python
                .transpose()
                .map_err(|err| FileConversionError::RequiresPython(err.line().clone(), err))?,
            size: file.size,
            upload_time_utc_ms: file.upload_time.map(Timestamp::as_millisecond),
            url: match split_scheme(&file.url) {
                Some(..) => FileLocation::AbsoluteUrl(UrlString::new(file.url)),
                None => FileLocation::RelativeUrl(base.clone(), file.url),
            },
            yanked: file.yanked,
        })
    }
}

/// While a registry file is generally a remote URL, it can also be a file if it comes from a directory flat indexes.
#[derive(Debug, Clone, PartialEq, Eq, Hash, rkyv::Archive, rkyv::Deserialize, rkyv::Serialize)]
#[rkyv(derive(Debug))]
pub enum FileLocation {
    /// URL relative to the base URL.
    RelativeUrl(SmallString, SmallString),
    /// Absolute URL.
    AbsoluteUrl(UrlString),
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
                    base: base.to_string(),
                    err,
                })?;
                let joined = base_url.join(path).map_err(|err| ToUrlError::InvalidJoin {
                    base: base.to_string(),
                    path: path.to_string(),
                    err,
                })?;
                Ok(joined)
            }
            FileLocation::AbsoluteUrl(ref absolute) => absolute.to_url(),
        }
    }
}

impl Display for FileLocation {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::RelativeUrl(_base, url) => Display::fmt(&url, f),
            Self::AbsoluteUrl(url) => Display::fmt(&url.0, f),
        }
    }
}

/// A [`Url`] represented as a `String`.
///
/// This type is not guaranteed to be a valid URL, and may error on conversion.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Deserialize,
    rkyv::Serialize,
)]
#[serde(transparent)]
#[rkyv(derive(Debug))]
pub struct UrlString(SmallString);

impl UrlString {
    /// Create a new [`UrlString`] from a [`String`].
    pub fn new(url: SmallString) -> Self {
        Self(url)
    }

    /// Converts a [`UrlString`] to a [`Url`].
    pub fn to_url(&self) -> Result<Url, ToUrlError> {
        Url::from_str(&self.0).map_err(|err| ToUrlError::InvalidAbsolute {
            absolute: self.0.to_string(),
            err,
        })
    }

    /// Return the [`UrlString`] with any query parameters and fragments removed.
    pub fn base_str(&self) -> &str {
        self.as_ref()
            .split_once('?')
            .or_else(|| self.as_ref().split_once('#'))
            .map(|(path, _)| path)
            .unwrap_or(self.as_ref())
    }

    /// Return the [`UrlString`] with any fragments removed.
    #[must_use]
    pub fn without_fragment(&self) -> Self {
        Self(
            self.as_ref()
                .split_once('#')
                .map(|(path, _)| path)
                .map(SmallString::from)
                .unwrap_or_else(|| self.0.clone()),
        )
    }
}

impl AsRef<str> for UrlString {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<Url> for UrlString {
    fn from(value: Url) -> Self {
        Self(value.as_str().into())
    }
}

impl From<&Url> for UrlString {
    fn from(value: &Url) -> Self {
        Self(value.as_str().into())
    }
}

impl Display for UrlString {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

/// An error that occurs when a [`FileLocation`] is not a valid URL.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ToUrlError {
    /// An error that occurs when the base URL in [`FileLocation::Relative`]
    /// could not be parsed as a valid URL.
    #[error("Could not parse base URL `{base}` as a valid URL")]
    InvalidBase {
        /// The base URL that could not be parsed as a valid URL.
        base: String,
        /// The underlying URL parse error.
        #[source]
        err: url::ParseError,
    },
    /// An error that occurs when the base URL could not be joined with
    /// the relative path in a [`FileLocation::Relative`].
    #[error("Could not join base URL `{base}` to relative path `{path}`")]
    InvalidJoin {
        /// The base URL that could not be parsed as a valid URL.
        base: String,
        /// The relative path segment.
        path: String,
        /// The underlying URL parse error.
        #[source]
        err: url::ParseError,
    },
    /// An error that occurs when the absolute URL in [`FileLocation::Absolute`]
    /// could not be parsed as a valid URL.
    #[error("Could not parse absolute URL `{absolute}` as a valid URL")]
    InvalidAbsolute {
        /// The absolute URL that could not be parsed as a valid URL.
        absolute: String,
        /// The underlying URL parse error.
        #[source]
        err: url::ParseError,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_str() {
        let url = UrlString("https://example.com/path?query#fragment".into());
        assert_eq!(url.base_str(), "https://example.com/path");

        let url = UrlString("https://example.com/path#fragment".into());
        assert_eq!(url.base_str(), "https://example.com/path");

        let url = UrlString("https://example.com/path".into());
        assert_eq!(url.base_str(), "https://example.com/path");
    }

    #[test]
    fn without_fragment() {
        let url = UrlString("https://example.com/path?query#fragment".into());
        assert_eq!(
            url.without_fragment(),
            UrlString("https://example.com/path?query".into())
        );

        let url = UrlString("https://example.com/path#fragment".into());
        assert_eq!(url.base_str(), "https://example.com/path");

        let url = UrlString("https://example.com/path".into());
        assert_eq!(url.base_str(), "https://example.com/path");
    }
}
