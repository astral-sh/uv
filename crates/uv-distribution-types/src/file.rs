use std::borrow::Cow;
use std::fmt::{self, Display, Formatter};
use std::marker::PhantomData;
use std::str::FromStr;
use std::sync::Arc;

use jiff::Timestamp;
use rkyv::Archived;
use serde::{Deserialize, Serialize};

use uv_pep440::{VersionSpecifiers, VersionSpecifiersParseError};
use uv_pep508::split_scheme;
use uv_pypi_types::{CoreMetadata, HashDigests, Yanked};
use uv_redacted::{SafeUrl, SafeUrlError};
use uv_small_str::SmallString;

/// Error converting [`uv_pypi_types::PypiFile`] to [`VerbatimFile`].
#[derive(Debug, thiserror::Error)]
pub enum FileConversionError {
    #[error("Failed to parse `requires-python`: `{0}`")]
    RequiresPython(String, #[source] VersionSpecifiersParseError),
}

/// Internal analog to [`uv_pypi_types::PypiFile`], with a canonical location by default.
///
/// Use [`VerbatimFile`] for locations as supplied by an index.
#[derive(Debug, Clone, PartialEq, Eq, Hash, rkyv::Archive, rkyv::Deserialize, rkyv::Serialize)]
#[rkyv(archive_bounds(Url::Archived: core::fmt::Debug), derive(Debug))]
pub struct File<Url = CanonicalFileLocation> {
    /// Hashes for separately available metadata, or an empty list when no hashes were provided.
    pub dist_info_metadata: Option<HashDigests>,
    pub filename: SmallString,
    pub hashes: HashDigests,
    pub requires_python: Option<Arc<VersionSpecifiers>>,
    pub size: Option<u64>,
    // N.B. We don't use a Jiff timestamp here because it's a little
    // annoying to do so with rkyv. Since we only use this field for doing
    // comparisons in testing, we just store it as a UTC timestamp in
    // milliseconds.
    pub upload_time_utc_ms: Option<i64>,
    pub url: Url,
    pub yanked: Option<Box<Yanked>>,
}

impl File<FileLocation> {
    /// `TryFrom` instead of `From` to filter out files with invalid requires python version specifiers
    pub fn try_from_pypi(
        file: uv_pypi_types::PypiFile,
        base: &SmallString,
    ) -> Result<Self, FileConversionError> {
        Ok(Self {
            dist_info_metadata: Self::dist_info_metadata(file.core_metadata),
            filename: file.filename,
            hashes: HashDigests::from(file.hashes),
            requires_python: file
                .requires_python
                .transpose()
                .map_err(|err| FileConversionError::RequiresPython(err.line().clone(), err))?,
            size: file.size,
            upload_time_utc_ms: file.upload_time.map(Timestamp::as_millisecond),
            url: FileLocation::new(file.url, base),
            yanked: file.yanked,
        })
    }

    fn dist_info_metadata(metadata: Option<CoreMetadata>) -> Option<HashDigests> {
        match metadata? {
            CoreMetadata::Bool(false) => None,
            CoreMetadata::Bool(true) => Some(HashDigests::empty()),
            CoreMetadata::Hashes(hashes) => Some(HashDigests::from(hashes)),
        }
    }
}

impl<Url> File<Url> {
    /// Replace the file URL while preserving the artifact metadata.
    pub fn map_url<Mapped>(self, map: impl FnOnce(Url) -> Mapped) -> File<Mapped> {
        File {
            dist_info_metadata: self.dist_info_metadata,
            filename: self.filename,
            hashes: self.hashes,
            requires_python: self.requires_python,
            size: self.size,
            upload_time_utc_ms: self.upload_time_utc_ms,
            url: map(self.url),
            yanked: self.yanked,
        }
    }
}

/// Provenance markers for [`FileLocation`].
pub mod provenance {
    /// A location for fetching an artifact, as supplied by an index.
    ///
    /// It may have been rewritten by a proxy and must not be used for artifact identity.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct Verbatim;

    /// An artifact's original location, suitable for artifact identity and lockfiles.
    ///
    /// This location is independent of any proxy URL rewriting used for downloads.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct Canonical;
}

/// An artifact location suitable for identity and lockfiles.
///
/// This is the artifact's original location, independent of any proxy URL rewriting used to fetch it.
/// Marking a [`FileLocation`] as canonical does not parse, normalize, or rewrite its URL.
pub type CanonicalFileLocation = FileLocation<provenance::Canonical>;

impl FileLocation<provenance::Canonical> {
    /// Mark an artifact location from a canonical source, such as a lockfile or direct index.
    pub fn from_location(location: FileLocation) -> Self {
        match location {
            FileLocation::RelativeUrl(base, path, _) => Self::relative(base, path),
            FileLocation::AbsoluteUrl(url, _) => Self::absolute(url),
        }
    }

    /// Store a parsed canonical artifact URL as an absolute location.
    pub fn from_url(url: SafeUrl) -> Self {
        Self::absolute(url.into())
    }
}

/// A file with its fetch location as supplied by an index, including any proxy URL rewriting.
///
/// Used when parsing and caching index responses. Its location must be mapped to the canonical
/// location before using it for artifact identity.
pub type VerbatimFile = File<FileLocation>;

/// An artifact location, either an absolute URL or a URL relative to an index page.
///
/// The provenance marker distinguishes [`provenance::Verbatim`] and [`provenance::Canonical`]
/// locations without adding bytes to the cached archive.
#[derive(Debug, Clone, PartialEq, Eq, Hash, rkyv::Archive, rkyv::Deserialize, rkyv::Serialize)]
#[rkyv(archive_bounds(Archived<PhantomData<Provenance>>: fmt::Debug), derive(Debug))]
pub enum FileLocation<Provenance = provenance::Verbatim> {
    /// URL relative to the base URL.
    RelativeUrl(SmallString, SmallString, PhantomData<Provenance>),
    /// Absolute URL.
    AbsoluteUrl(UrlString, PhantomData<Provenance>),
}

impl FileLocation {
    /// Parse a relative or absolute URL on a page with a base URL.
    ///
    /// This follows the HTML semantics where a link on a page is resolved relative to the URL of
    /// that page.
    pub fn new(url: SmallString, base: &SmallString) -> Self {
        match split_scheme(&url) {
            Some(..) => Self::absolute(UrlString::new(url)),
            None => Self::relative(base.clone(), url),
        }
    }
}

impl<Provenance> FileLocation<Provenance> {
    /// Store an absolute URL without parsing it.
    pub fn absolute(url: UrlString) -> Self {
        Self::AbsoluteUrl(url, PhantomData)
    }

    /// Store a relative URL and its base without parsing them.
    fn relative(base: SmallString, url: SmallString) -> Self {
        Self::RelativeUrl(base, url, PhantomData)
    }

    /// Returns the final raw URL path component after removing any query or fragment.
    ///
    /// The filename is not percent-decoded.
    pub fn raw_filename(&self) -> &str {
        let path = match self {
            Self::RelativeUrl(_, path, _) => path.as_ref(),
            Self::AbsoluteUrl(url, _) => url.as_ref(),
        };
        let path = path.split_once(['?', '#']).map_or(path, |(path, _)| path);
        path.rsplit_once('/').map_or(path, |(_, filename)| filename)
    }

    /// Convert this location to a URL.
    ///
    /// A relative URL has its base joined to the path. An absolute URL is parsed as-is.
    ///
    /// # Errors
    ///
    /// This returns an error if parsing the base or absolute URL fails, or if the relative path
    /// cannot be joined to the base URL.
    pub fn to_url(&self) -> Result<SafeUrl, ToUrlError> {
        match self {
            Self::RelativeUrl(base, path, _) => {
                let base_url = SafeUrl::parse(base).map_err(|err| ToUrlError::InvalidBase {
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
            Self::AbsoluteUrl(absolute, _) => absolute.to_url(),
        }
    }
}

impl<Provenance> Display for FileLocation<Provenance> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::RelativeUrl(_base, url, _) => Display::fmt(&url, f),
            Self::AbsoluteUrl(url, _) => Display::fmt(&url.0, f),
        }
    }
}

/// A URL represented as a [`SmallString`].
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
    /// Create a new [`UrlString`] from a [`SmallString`].
    fn new(url: SmallString) -> Self {
        Self(url)
    }

    /// Converts a [`UrlString`] to a [`SafeUrl`].
    pub fn to_url(&self) -> Result<SafeUrl, ToUrlError> {
        SafeUrl::from_str(&self.0).map_err(|err| ToUrlError::InvalidAbsolute {
            absolute: self.0.to_string(),
            err,
        })
    }

    /// Return the [`UrlString`] with any query parameters and fragments removed.
    pub fn base_str(&self) -> &str {
        self.as_ref()
            .split_once(['?', '#'])
            .map(|(path, _)| path)
            .unwrap_or(self.as_ref())
    }

    /// Return the [`UrlString`] (as a [`Cow`]) with any fragments removed.
    #[must_use]
    pub fn without_fragment(&self) -> Cow<'_, Self> {
        self.as_ref()
            .split_once('#')
            .map(|(path, _)| Cow::Owned(Self(SmallString::from(path))))
            .unwrap_or(Cow::Borrowed(self))
    }
}

impl AsRef<str> for UrlString {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<SafeUrl> for UrlString {
    fn from(value: SafeUrl) -> Self {
        Self(value.as_str().into())
    }
}

impl From<&SafeUrl> for UrlString {
    fn from(value: &SafeUrl) -> Self {
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
    /// An error that occurs when the base URL in [`FileLocation::RelativeUrl`]
    /// could not be parsed as a valid URL.
    #[error("Could not parse base URL `{base}` as a valid URL")]
    InvalidBase {
        /// The base URL that could not be parsed as a valid URL.
        base: String,
        /// The underlying URL parse error.
        #[source]
        err: SafeUrlError,
    },
    /// An error that occurs when the base URL could not be joined with
    /// the relative path in a [`FileLocation::RelativeUrl`].
    #[error("Could not join base URL `{base}` to relative path `{path}`")]
    InvalidJoin {
        /// The base URL that could not be parsed as a valid URL.
        base: String,
        /// The relative path segment.
        path: String,
        /// The underlying URL parse error.
        #[source]
        err: SafeUrlError,
    },
    /// An error that occurs when the absolute URL in [`FileLocation::AbsoluteUrl`]
    /// could not be parsed as a valid URL.
    #[error("Could not parse absolute URL `{absolute}` as a valid URL")]
    InvalidAbsolute {
        /// The absolute URL that could not be parsed as a valid URL.
        absolute: String,
        /// The underlying URL parse error.
        #[source]
        err: SafeUrlError,
    },
}

#[cfg(test)]
mod tests {
    use std::assert_matches;

    use super::*;

    #[test]
    fn raw_filename() {
        let base = SmallString::from("https://example.com/simple/");

        let location = FileLocation::new(
            SmallString::from("files/example%20pkg.whl?download=1#fragment"),
            &base,
        );
        assert_eq!(location.raw_filename(), "example%20pkg.whl");

        let location = FileLocation::new(
            SmallString::from("https://files.example.com/example.whl#sha256=digest"),
            &base,
        );
        assert_eq!(location.raw_filename(), "example.whl");
    }

    #[test]
    fn base_str() {
        let url = UrlString("https://example.com/path?query#fragment".into());
        assert_eq!(url.base_str(), "https://example.com/path");

        let url = UrlString("https://example.com/path#fragment".into());
        assert_eq!(url.base_str(), "https://example.com/path");

        let url = UrlString("https://example.com/path#fragment?query".into());
        assert_eq!(url.base_str(), "https://example.com/path");

        let url = UrlString("https://example.com/path#fragment/part?query".into());
        assert_eq!(url.base_str(), "https://example.com/path");

        let url = UrlString("https://example.com/path".into());
        assert_eq!(url.base_str(), "https://example.com/path");
    }

    #[test]
    fn without_fragment() {
        // Borrows a URL without a fragment
        let url = UrlString("https://example.com/path".into());
        assert_eq!(&*url.without_fragment(), &url);
        assert_matches!(url.without_fragment(), Cow::Borrowed(_));

        // Removes the fragment if present on the URL
        let url = UrlString("https://example.com/path?query#fragment".into());
        assert_eq!(
            &*url.without_fragment(),
            &UrlString("https://example.com/path?query".into())
        );
        assert_matches!(url.without_fragment(), Cow::Owned(_));
    }
}
