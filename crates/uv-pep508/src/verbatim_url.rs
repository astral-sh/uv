use std::borrow::Cow;
use std::cmp::Ordering;
use std::fmt::{Debug, Display};
use std::hash::Hash;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use arcstr::ArcStr;
use regex::Regex;
use thiserror::Error;
use url::{ParseError, Url};
use uv_cache_key::{CacheKey, CacheKeyHasher};

#[cfg_attr(not(feature = "non-pep508-extensions"), allow(unused_imports))]
use uv_fs::{normalize_absolute_path, normalize_url_path};
use uv_redacted::DisplaySafeUrl;

use crate::Pep508Url;

/// A wrapper around [`Url`] that preserves the original string.
///
/// The original string is not preserved after serialization/deserialization.
#[derive(Debug, Clone, Eq)]
pub struct VerbatimUrl {
    /// The parsed URL.
    url: DisplaySafeUrl,
    /// The URL as it was provided by the user.
    ///
    /// Even if originally set, this will be [`None`] after
    /// serialization/deserialization.
    given: Option<ArcStr>,
}

impl Hash for VerbatimUrl {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.url.hash(state);
    }
}

impl CacheKey for VerbatimUrl {
    fn cache_key(&self, state: &mut CacheKeyHasher) {
        self.url.as_str().cache_key(state);
    }
}

impl PartialEq for VerbatimUrl {
    fn eq(&self, other: &Self) -> bool {
        self.url == other.url
    }
}

impl VerbatimUrl {
    /// Create a [`VerbatimUrl`] from a [`Url`].
    pub fn from_url(url: DisplaySafeUrl) -> Self {
        Self { url, given: None }
    }

    /// Parse a URL from a string.
    pub fn parse_url(given: impl AsRef<str>) -> Result<Self, ParseError> {
        let url = Url::parse(given.as_ref())?;
        Ok(Self {
            url: DisplaySafeUrl::from(url),
            given: None,
        })
    }

    /// Convert a [`VerbatimUrl`] from a path or a URL.
    ///
    /// If no root directory is provided, relative paths are resolved against the current working
    /// directory.
    #[cfg(feature = "non-pep508-extensions")] // PEP 508 arguably only allows absolute file URLs.
    pub fn from_url_or_path(
        input: &str,
        root_dir: Option<&Path>,
    ) -> Result<Self, VerbatimUrlError> {
        let url = match split_scheme(input) {
            Some((scheme, ..)) => {
                match Scheme::parse(scheme) {
                    Some(_) => {
                        // Ex) `https://pypi.org/simple`
                        Self::parse_url(input)?
                    }
                    None => {
                        // Ex) `C:\Users\user\index`
                        if let Some(root_dir) = root_dir {
                            Self::from_path(input, root_dir)?
                        } else {
                            let absolute_path = std::path::absolute(input).map_err(|err| {
                                VerbatimUrlError::Absolute(input.to_string(), err)
                            })?;
                            Self::from_absolute_path(absolute_path)?
                        }
                    }
                }
            }
            None => {
                // Ex) `/Users/user/index`
                if let Some(root_dir) = root_dir {
                    Self::from_path(input, root_dir)?
                } else {
                    let absolute_path = std::path::absolute(input)
                        .map_err(|err| VerbatimUrlError::Absolute(input.to_string(), err))?;
                    Self::from_absolute_path(absolute_path)?
                }
            }
        };
        Ok(url.with_given(input))
    }

    /// Parse a URL from an absolute or relative path.
    #[cfg(feature = "non-pep508-extensions")] // PEP 508 arguably only allows absolute file URLs.
    pub fn from_path(
        path: impl AsRef<Path>,
        base_dir: impl AsRef<Path>,
    ) -> Result<Self, VerbatimUrlError> {
        debug_assert!(base_dir.as_ref().is_absolute(), "base dir must be absolute");
        let path = path.as_ref();

        // Convert the path to an absolute path, if necessary.
        let path = if path.is_absolute() {
            Cow::Borrowed(path)
        } else {
            Cow::Owned(base_dir.as_ref().join(path))
        };

        let path = normalize_absolute_path(&path)
            .map_err(|err| VerbatimUrlError::Normalization(path.to_path_buf(), err))?;

        // Extract the fragment, if it exists.
        let (path, fragment) = split_fragment(&path);

        // Convert to a URL.
        let mut url = DisplaySafeUrl::from_file_path(path.clone())
            .map_err(|()| VerbatimUrlError::UrlConversion(path.to_path_buf()))?;

        // Set the fragment, if it exists.
        if let Some(fragment) = fragment {
            url.set_fragment(Some(fragment));
        }

        Ok(Self { url, given: None })
    }

    /// Parse a URL from an absolute path.
    pub fn from_absolute_path(path: impl AsRef<Path>) -> Result<Self, VerbatimUrlError> {
        let path = path.as_ref();

        // Error if the path is relative.
        let path = if path.is_absolute() {
            path
        } else {
            return Err(VerbatimUrlError::WorkingDirectory(path.to_path_buf()));
        };

        // Normalize the path.
        let path = normalize_absolute_path(path)
            .map_err(|err| VerbatimUrlError::Normalization(path.to_path_buf(), err))?;

        // Extract the fragment, if it exists.
        let (path, fragment) = split_fragment(&path);

        // Convert to a URL.
        let mut url = DisplaySafeUrl::from_file_path(path.clone())
            .unwrap_or_else(|()| panic!("path is absolute: {}", path.display()));

        // Set the fragment, if it exists.
        if let Some(fragment) = fragment {
            url.set_fragment(Some(fragment));
        }

        Ok(Self { url, given: None })
    }

    /// Parse a URL from a normalized path.
    ///
    /// Like [`VerbatimUrl::from_absolute_path`], but skips the normalization step.
    pub fn from_normalized_path(path: impl AsRef<Path>) -> Result<Self, VerbatimUrlError> {
        let path = path.as_ref();

        // Error if the path is relative.
        let path = if path.is_absolute() {
            path
        } else {
            return Err(VerbatimUrlError::WorkingDirectory(path.to_path_buf()));
        };

        // Extract the fragment, if it exists.
        let (path, fragment) = split_fragment(path);

        // Convert to a URL.
        let mut url = DisplaySafeUrl::from(
            Url::from_file_path(path.clone())
                .unwrap_or_else(|()| panic!("path is absolute: {}", path.display())),
        );

        // Set the fragment, if it exists.
        if let Some(fragment) = fragment {
            url.set_fragment(Some(fragment));
        }

        Ok(Self { url, given: None })
    }

    /// Set the verbatim representation of the URL.
    #[must_use]
    pub fn with_given(self, given: impl AsRef<str>) -> Self {
        Self {
            given: Some(ArcStr::from(given.as_ref())),
            ..self
        }
    }

    /// Return the original string as given by the user, if available.
    pub fn given(&self) -> Option<&str> {
        self.given.as_deref()
    }

    /// Return the underlying [`DisplaySafeUrl`].
    pub fn raw(&self) -> &DisplaySafeUrl {
        &self.url
    }

    /// Convert a [`VerbatimUrl`] into a [`DisplaySafeUrl`].
    pub fn to_url(&self) -> DisplaySafeUrl {
        self.url.clone()
    }

    /// Convert a [`VerbatimUrl`] into a [`DisplaySafeUrl`].
    pub fn into_url(self) -> DisplaySafeUrl {
        self.url
    }

    /// Return the underlying [`Path`], if the URL is a file URL.
    #[cfg(feature = "non-pep508-extensions")]
    pub fn as_path(&self) -> Result<PathBuf, VerbatimUrlError> {
        self.url
            .to_file_path()
            .map_err(|()| VerbatimUrlError::UrlConversion(self.url.to_file_path().unwrap()))
    }
}

impl Ord for VerbatimUrl {
    fn cmp(&self, other: &Self) -> Ordering {
        self.url.cmp(&other.url)
    }
}

impl PartialOrd for VerbatimUrl {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl std::str::FromStr for VerbatimUrl {
    type Err = VerbatimUrlError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self::parse_url(s).map(|url| url.with_given(s))?)
    }
}

impl std::fmt::Display for VerbatimUrl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.url, f)
    }
}

impl Deref for VerbatimUrl {
    type Target = DisplaySafeUrl;

    fn deref(&self) -> &Self::Target {
        &self.url
    }
}

impl From<Url> for VerbatimUrl {
    fn from(url: Url) -> Self {
        Self::from_url(DisplaySafeUrl::from(url))
    }
}

impl From<DisplaySafeUrl> for VerbatimUrl {
    fn from(url: DisplaySafeUrl) -> Self {
        Self::from_url(url)
    }
}

impl From<VerbatimUrl> for Url {
    fn from(url: VerbatimUrl) -> Self {
        Self::from(url.url)
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for VerbatimUrl {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.url.serialize(serializer)
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for VerbatimUrl {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let url = DisplaySafeUrl::deserialize(deserializer)?;
        Ok(Self::from_url(url))
    }
}

impl Pep508Url for VerbatimUrl {
    type Err = VerbatimUrlError;

    /// Create a [`VerbatimUrl`] to represent a PEP 508 requirement.
    ///
    /// Any environment variables in the URL will be expanded.
    #[cfg_attr(not(feature = "non-pep508-extensions"), allow(unused_variables))]
    fn parse_url(url: &str, working_dir: Option<&Path>) -> Result<Self, Self::Err> {
        // Expand environment variables in the URL.
        let expanded = expand_env_vars(url);

        if let Some((scheme, path)) = split_scheme(&expanded) {
            match Scheme::parse(scheme) {
                // Ex) `file:///home/ferris/project/scripts/...`, `file://localhost/home/ferris/project/scripts/...`, or `file:../ferris/`
                Some(Scheme::File) => {
                    // Strip the leading slashes, along with the `localhost` host, if present.

                    // Transform, e.g., `/C:/Users/ferris/wheel-0.42.0.tar.gz` to `C:\Users\ferris\wheel-0.42.0.tar.gz`.
                    #[cfg(feature = "non-pep508-extensions")]
                    {
                        let path = strip_host(path);

                        let path = normalize_url_path(path);

                        if let Some(working_dir) = working_dir {
                            return Ok(Self::from_path(path.as_ref(), working_dir)?.with_given(url));
                        }

                        Ok(Self::from_absolute_path(path.as_ref())?.with_given(url))
                    }
                    #[cfg(not(feature = "non-pep508-extensions"))]
                    Ok(Self::parse_url(expanded)?.with_given(url))
                }

                // Ex) `https://download.pytorch.org/whl/torch_stable.html`
                Some(_) => {
                    // Ex) `https://download.pytorch.org/whl/torch_stable.html`
                    Ok(Self::parse_url(expanded.as_ref())?.with_given(url))
                }

                // Ex) `C:\Users\ferris\wheel-0.42.0.tar.gz`
                _ => {
                    #[cfg(feature = "non-pep508-extensions")]
                    {
                        if let Some(working_dir) = working_dir {
                            return Ok(
                                Self::from_path(expanded.as_ref(), working_dir)?.with_given(url)
                            );
                        }

                        Ok(Self::from_absolute_path(expanded.as_ref())?.with_given(url))
                    }
                    #[cfg(not(feature = "non-pep508-extensions"))]
                    Err(Self::Err::NotAUrl(expanded.to_string()))
                }
            }
        } else {
            // Ex) `../editable/`
            #[cfg(feature = "non-pep508-extensions")]
            {
                if let Some(working_dir) = working_dir {
                    return Ok(Self::from_path(expanded.as_ref(), working_dir)?.with_given(url));
                }

                Ok(Self::from_absolute_path(expanded.as_ref())?.with_given(url))
            }

            #[cfg(not(feature = "non-pep508-extensions"))]
            Err(Self::Err::NotAUrl(expanded.to_string()))
        }
    }

    fn displayable_with_credentials(&self) -> impl Display {
        self.url.displayable_with_credentials()
    }
}

/// An error that can occur when parsing a [`VerbatimUrl`].
#[derive(Error, Debug)]
pub enum VerbatimUrlError {
    /// Failed to parse a URL.
    #[error(transparent)]
    Url(#[from] ParseError),

    /// Received a relative path, but no working directory was provided.
    #[error("relative path without a working directory: {0}")]
    WorkingDirectory(PathBuf),

    /// Received a path that could not be converted to a URL.
    #[error("path could not be converted to a URL: {0}")]
    UrlConversion(PathBuf),

    /// Received a path that could not be normalized.
    #[error("path could not be normalized: {0}")]
    Normalization(PathBuf, #[source] std::io::Error),

    /// Received a path that could not be converted to an absolute path.
    #[error("path could not be converted to an absolute path: {0}")]
    Absolute(String, #[source] std::io::Error),

    /// Received a path that could not be normalized.
    #[cfg(not(feature = "non-pep508-extensions"))]
    #[error("Not a URL (missing scheme): {0}")]
    NotAUrl(String),
}

/// Expand all available environment variables.
///
/// This is modeled off of pip's environment variable expansion, which states:
///
///   The only allowed format for environment variables defined in the
///   requirement file is `${MY_VARIABLE_1}` to ensure two things:
///
///   1. Strings that contain a `$` aren't accidentally (partially) expanded.
///   2. Ensure consistency across platforms for requirement files.
///
///   ...
///
///   Valid characters in variable names follow the `POSIX standard
///   <http://pubs.opengroup.org/onlinepubs/9699919799/>`_ and are limited
///   to uppercase letter, digits and the `_` (underscore).
pub fn expand_env_vars(s: &str) -> Cow<'_, str> {
    // Generate the project root, to be used via the `${PROJECT_ROOT}`
    // environment variable.
    static PROJECT_ROOT_FRAGMENT: LazyLock<String> = LazyLock::new(|| {
        let project_root = std::env::current_dir().unwrap();
        project_root.to_string_lossy().to_string()
    });

    static RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?P<var>\$\{(?P<name>[A-Z0-9_]+)})").unwrap());

    RE.replace_all(s, |caps: &regex::Captures<'_>| {
        let name = caps.name("name").unwrap().as_str();
        std::env::var(name).unwrap_or_else(|_| match name {
            "PROJECT_ROOT" => PROJECT_ROOT_FRAGMENT.to_string(),
            _ => caps["var"].to_owned(),
        })
    })
}

/// Like [`Url::parse`], but only splits the scheme. Derived from the `url` crate.
pub fn split_scheme(s: &str) -> Option<(&str, &str)> {
    /// <https://url.spec.whatwg.org/#c0-controls-and-space>
    #[inline]
    fn c0_control_or_space(ch: char) -> bool {
        ch <= ' ' // U+0000 to U+0020
    }

    /// <https://url.spec.whatwg.org/#ascii-alpha>
    #[inline]
    fn ascii_alpha(ch: char) -> bool {
        ch.is_ascii_alphabetic()
    }

    // Trim control characters and spaces from the start and end.
    let s = s.trim_matches(c0_control_or_space);
    if s.is_empty() || !s.starts_with(ascii_alpha) {
        return None;
    }

    // Find the `:` following any alpha characters.
    let mut iter = s.char_indices();
    let end = loop {
        match iter.next() {
            Some((_i, 'a'..='z' | 'A'..='Z' | '0'..='9' | '+' | '-' | '.')) => {}
            Some((i, ':')) => break i,
            _ => return None,
        }
    };

    let scheme = &s[..end];
    let rest = &s[end + 1..];
    Some((scheme, rest))
}

/// Strip the `file://localhost/` host from a file path.
pub fn strip_host(path: &str) -> &str {
    // Ex) `file://localhost/...`.
    if let Some(path) = path
        .strip_prefix("//localhost")
        .filter(|path| path.starts_with('/'))
    {
        return path;
    }

    // Ex) `file:///...`.
    if let Some(path) = path.strip_prefix("//") {
        return path;
    }

    path
}

/// Returns `true` if a URL looks like a reference to a Git repository (e.g., `https://github.com/user/repo.git`).
pub fn looks_like_git_repository(url: &Url) -> bool {
    matches!(
        url.host_str(),
        Some("github.com" | "gitlab.com" | "bitbucket.org")
    ) && Path::new(url.path())
        .extension()
        .is_none_or(|ext| ext.eq_ignore_ascii_case("git"))
        && url
            .path_segments()
            .is_some_and(|segments| segments.count() == 2)
}

/// Split the fragment from a URL.
///
/// For example, given `file:///home/ferris/project/scripts#hash=somehash`, returns
/// `("/home/ferris/project/scripts", Some("hash=somehash"))`.
fn split_fragment(path: &Path) -> (Cow<Path>, Option<&str>) {
    let Some(s) = path.to_str() else {
        return (Cow::Borrowed(path), None);
    };

    let Some((path, fragment)) = s.split_once('#') else {
        return (Cow::Borrowed(path), None);
    };

    (Cow::Owned(PathBuf::from(path)), Some(fragment))
}

/// A supported URL scheme for PEP 508 direct-URL requirements.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scheme {
    /// `file://...`
    File,
    /// `git+{transport}://...` as git supports arbitrary transports through gitremote-helpers
    Git(String),
    /// `bzr+http://...`
    BzrHttp,
    /// `bzr+https://...`
    BzrHttps,
    /// `bzr+ssh://...`
    BzrSsh,
    /// `bzr+sftp://...`
    BzrSftp,
    /// `bzr+ftp://...`
    BzrFtp,
    /// `bzr+lp://...`
    BzrLp,
    /// `bzr+file://...`
    BzrFile,
    /// `hg+file://...`
    HgFile,
    /// `hg+http://...`
    HgHttp,
    /// `hg+https://...`
    HgHttps,
    /// `hg+ssh://...`
    HgSsh,
    /// `hg+static-http://...`
    HgStaticHttp,
    /// `svn+ssh://...`
    SvnSsh,
    /// `svn+http://...`
    SvnHttp,
    /// `svn+https://...`
    SvnHttps,
    /// `svn+svn://...`
    SvnSvn,
    /// `svn+file://...`
    SvnFile,
    /// `http://...`
    Http,
    /// `https://...`
    Https,
}

impl Scheme {
    /// Determine the [`Scheme`] from the given string, if possible.
    pub fn parse(s: &str) -> Option<Self> {
        if let Some(("git", transport)) = s.split_once('+') {
            return Some(Self::Git(transport.into()));
        }
        match s {
            "file" => Some(Self::File),
            "bzr+http" => Some(Self::BzrHttp),
            "bzr+https" => Some(Self::BzrHttps),
            "bzr+ssh" => Some(Self::BzrSsh),
            "bzr+sftp" => Some(Self::BzrSftp),
            "bzr+ftp" => Some(Self::BzrFtp),
            "bzr+lp" => Some(Self::BzrLp),
            "bzr+file" => Some(Self::BzrFile),
            "hg+file" => Some(Self::HgFile),
            "hg+http" => Some(Self::HgHttp),
            "hg+https" => Some(Self::HgHttps),
            "hg+ssh" => Some(Self::HgSsh),
            "hg+static-http" => Some(Self::HgStaticHttp),
            "svn+ssh" => Some(Self::SvnSsh),
            "svn+http" => Some(Self::SvnHttp),
            "svn+https" => Some(Self::SvnHttps),
            "svn+svn" => Some(Self::SvnSvn),
            "svn+file" => Some(Self::SvnFile),
            "http" => Some(Self::Http),
            "https" => Some(Self::Https),
            _ => None,
        }
    }

    /// Returns `true` if the scheme is a file scheme.
    pub fn is_file(self) -> bool {
        matches!(self, Self::File)
    }
}

impl std::fmt::Display for Scheme {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::File => write!(f, "file"),
            Self::Git(transport) => write!(f, "git+{transport}"),
            Self::BzrHttp => write!(f, "bzr+http"),
            Self::BzrHttps => write!(f, "bzr+https"),
            Self::BzrSsh => write!(f, "bzr+ssh"),
            Self::BzrSftp => write!(f, "bzr+sftp"),
            Self::BzrFtp => write!(f, "bzr+ftp"),
            Self::BzrLp => write!(f, "bzr+lp"),
            Self::BzrFile => write!(f, "bzr+file"),
            Self::HgFile => write!(f, "hg+file"),
            Self::HgHttp => write!(f, "hg+http"),
            Self::HgHttps => write!(f, "hg+https"),
            Self::HgSsh => write!(f, "hg+ssh"),
            Self::HgStaticHttp => write!(f, "hg+static-http"),
            Self::SvnSsh => write!(f, "svn+ssh"),
            Self::SvnHttp => write!(f, "svn+http"),
            Self::SvnHttps => write!(f, "svn+https"),
            Self::SvnSvn => write!(f, "svn+svn"),
            Self::SvnFile => write!(f, "svn+file"),
            Self::Http => write!(f, "http"),
            Self::Https => write!(f, "https"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scheme() {
        assert_eq!(
            split_scheme("file:///home/ferris/project/scripts"),
            Some(("file", "///home/ferris/project/scripts"))
        );
        assert_eq!(
            split_scheme("file:home/ferris/project/scripts"),
            Some(("file", "home/ferris/project/scripts"))
        );
        assert_eq!(
            split_scheme("https://example.com"),
            Some(("https", "//example.com"))
        );
        assert_eq!(split_scheme("https:"), Some(("https", "")));
    }

    #[test]
    fn fragment() {
        assert_eq!(
            split_fragment(Path::new(
                "file:///home/ferris/project/scripts#hash=somehash"
            )),
            (
                Cow::Owned(PathBuf::from("file:///home/ferris/project/scripts")),
                Some("hash=somehash")
            )
        );
        assert_eq!(
            split_fragment(Path::new("file:home/ferris/project/scripts#hash=somehash")),
            (
                Cow::Owned(PathBuf::from("file:home/ferris/project/scripts")),
                Some("hash=somehash")
            )
        );
        assert_eq!(
            split_fragment(Path::new("/home/ferris/project/scripts#hash=somehash")),
            (
                Cow::Owned(PathBuf::from("/home/ferris/project/scripts")),
                Some("hash=somehash")
            )
        );
        assert_eq!(
            split_fragment(Path::new("file:///home/ferris/project/scripts")),
            (
                Cow::Borrowed(Path::new("file:///home/ferris/project/scripts")),
                None
            )
        );
        assert_eq!(
            split_fragment(Path::new("")),
            (Cow::Borrowed(Path::new("")), None)
        );
    }

    #[test]
    fn git_repository() {
        let url = Url::parse("https://github.com/user/repo.git").unwrap();
        assert!(looks_like_git_repository(&url));

        let url = Url::parse("https://gitlab.com/user/repo.git").unwrap();
        assert!(looks_like_git_repository(&url));

        let url = Url::parse("https://bitbucket.org/user/repo.git").unwrap();
        assert!(looks_like_git_repository(&url));

        let url = Url::parse("https://github.com/user/repo").unwrap();
        assert!(looks_like_git_repository(&url));

        let url = Url::parse("https://example.com/user/repo.git").unwrap();
        assert!(!looks_like_git_repository(&url));

        let url = Url::parse("https://github.com/user").unwrap();
        assert!(!looks_like_git_repository(&url));

        let url = Url::parse("https://github.com/user/repo.zip").unwrap();
        assert!(!looks_like_git_repository(&url));

        let url = Url::parse("https://github.com/").unwrap();
        assert!(!looks_like_git_repository(&url));

        let url = Url::parse("").unwrap_err();
        assert_eq!(url.to_string(), "relative URL without a base");

        let url = Url::parse("github.com/user/repo.git").unwrap_err();
        assert_eq!(url.to_string(), "relative URL without a base");

        let url = Url::parse("https://github.com/user/repo/extra.git").unwrap();
        assert!(!looks_like_git_repository(&url));

        let url = Url::parse("https://github.com/user/repo.GIT").unwrap();
        assert!(looks_like_git_repository(&url));

        let url = Url::parse("https://github.com/user/repo.git?foo=bar").unwrap();
        assert!(looks_like_git_repository(&url));

        let url = Url::parse("https://github.com/user/repo.git#readme").unwrap();
        assert!(looks_like_git_repository(&url));

        let url = Url::parse("https://github.com/pypa/pip/archive/1.3.1.zip#sha1=da9234ee9982d4bbb3c72346a6de940a148ea686").unwrap();
        assert!(!looks_like_git_repository(&url));
    }
}
