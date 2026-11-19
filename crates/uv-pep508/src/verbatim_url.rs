use std::borrow::Cow;
use std::cmp::Ordering;
use std::fmt::{Debug, Display};
use std::hash::Hash;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use arcstr::ArcStr;
use thiserror::Error;
#[cfg(feature = "unsafe-url")]
#[expect(clippy::disallowed_types, reason = "Supports the opt-in raw URL API")]
use url::Url;
use uv_cache_key::{CacheKey, CacheKeyHasher};

#[cfg_attr(not(feature = "non-pep508-extensions"), allow(unused_imports))]
use uv_fs::{normalize_absolute_path, normalize_url_path};
use uv_redacted::policy::{SerializationPolicy, Unserializable};
use uv_redacted::{SafeUrl, SafeUrlError};

use crate::Pep508Url;

/// A wrapper around [`SafeUrl`] that preserves the original string.
///
/// The `From<url::Url>` implementation requires the `unsafe-url` feature.
///
/// The serialization policy is stored on the parsed URL. A [`VerbatimUrl`] with
/// the default [`Unserializable`] policy does not implement [`serde::Serialize`].
/// With the `serde` feature, URLs with [`uv_redacted::policy::Stripped`] or
/// [`uv_redacted::policy::Raw`] serialize the normalized URL and omit the original input.
///
/// ```compile_fail
/// use uv_pep508::VerbatimUrl;
///
/// fn require_serialize<T: serde::Serialize>() {}
/// // `VerbatimUrl` uses the `Unserializable` policy by default.
/// require_serialize::<VerbatimUrl>();
/// ```
///
/// `serialize` method calls through [`Deref`] also require a serializing policy:
///
/// ```compile_fail
/// use serde::Serialize;
/// use uv_pep508::VerbatimUrl;
///
/// let url = VerbatimUrl::parse_url("https://user:password@example.com")?;
/// url.serialize(serde_json::value::Serializer)?;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
///
/// Internal serialization requires [`uv_redacted::policy::Raw`]:
///
/// ```compile_fail
/// use uv_pep508::VerbatimUrl;
/// use uv_redacted::policy::Stripped;
///
/// let url = VerbatimUrl::parse_url("https://user:password@example.com")?
///     .with_policy(Stripped);
/// url.serialize_internal(serde_json::value::Serializer)?;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[cfg_attr(
    not(feature = "unsafe-url"),
    doc = r#"
Without `unsafe-url`, raw URLs cannot be converted directly to [`VerbatimUrl`]:

```compile_fail
use uv_pep508::VerbatimUrl;

let url = url::Url::parse("https://example.com").unwrap();
let _: VerbatimUrl = url.into();
```
"#
)]
#[derive(Clone, Eq)]
pub struct VerbatimUrl<P: SerializationPolicy = Unserializable> {
    /// The parsed URL.
    url: SafeUrl<P>,
    /// The URL as it was provided by the user.
    ///
    /// This is [`None`] when the URL is deserialized.
    given: Option<ArcStr>,
    /// Given value is a [`Pep508Url`] which contained variable references which were successfully
    /// expanded.
    expanded: bool,
    /// Whether this URL should be represented by a relative path regardless of its input.
    force_relative: bool,
}

impl<P: SerializationPolicy> Debug for VerbatimUrl<P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let given = self.given.as_deref().map(|given| {
            SafeUrl::parse(given).map_or_else(
                |_| Cow::Borrowed(given),
                |url| {
                    let redacted = url.to_string();
                    if redacted == url.displayable_with_credentials().to_string() {
                        Cow::Borrowed(given)
                    } else {
                        Cow::Owned(redacted)
                    }
                },
            )
        });

        f.debug_struct("VerbatimUrl")
            .field("url", &self.url)
            .field("given", &given)
            .field("expanded", &self.expanded)
            .field("force_relative", &self.force_relative)
            .finish()
    }
}

impl<P: SerializationPolicy> Hash for VerbatimUrl<P> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.url.hash(state);
    }
}

impl<P: SerializationPolicy> CacheKey for VerbatimUrl<P> {
    fn cache_key(&self, state: &mut CacheKeyHasher) {
        self.url.as_str().cache_key(state);
    }
}

impl<P: SerializationPolicy> PartialEq for VerbatimUrl<P> {
    fn eq(&self, other: &Self) -> bool {
        self.url == other.url
    }
}

impl VerbatimUrl {
    /// Parse a URL from a string.
    pub fn parse_url(given: impl AsRef<str>) -> Result<Self, VerbatimUrlError> {
        let given = given.as_ref();
        let url = SafeUrl::parse(given)?;

        Ok(Self {
            url,
            given: None,
            expanded: false,
            force_relative: false,
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
                            Self::from_path_with_fragment(input, Some(root_dir))?
                        } else {
                            let absolute_path = std::path::absolute(input).map_err(|err| {
                                VerbatimUrlError::Absolute(input.to_string(), err)
                            })?;
                            Self::from_path_with_fragment(absolute_path, None)?
                        }
                    }
                }
            }
            None => {
                // Ex) `/Users/user/index`
                if let Some(root_dir) = root_dir {
                    Self::from_path_with_fragment(input, Some(root_dir))?
                } else {
                    let absolute_path = std::path::absolute(input)
                        .map_err(|err| VerbatimUrlError::Absolute(input.to_string(), err))?;
                    Self::from_path_with_fragment(absolute_path, None)?
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

        // Convert to a URL.
        let url =
            SafeUrl::from_file_path(&path).map_err(|()| VerbatimUrlError::UrlConversion(path))?;

        Ok(Self {
            url,
            given: None,
            expanded: false,
            force_relative: false,
        })
    }

    /// Parse a URL from an absolute or relative path, including a URL fragment.
    #[cfg(feature = "non-pep508-extensions")]
    pub(crate) fn from_path_with_fragment(
        path: impl AsRef<Path>,
        base_dir: Option<&Path>,
    ) -> Result<Self, VerbatimUrlError> {
        let (path, fragment) = split_fragment(path.as_ref());
        let url = if let Some(base_dir) = base_dir {
            Self::from_path(path, base_dir)?
        } else {
            Self::from_absolute_path(path)?
        };
        Ok(url.with_url_fragment(fragment))
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

        // Convert to a URL.
        let url = SafeUrl::from_file_path(&path)
            .unwrap_or_else(|()| panic!("path is absolute: {}", path.display()));

        Ok(Self {
            url,
            given: None,
            expanded: false,
            force_relative: false,
        })
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

        // Convert to a URL.
        let url = SafeUrl::from_file_path(path)
            .unwrap_or_else(|()| panic!("path is absolute: {}", path.display()));

        Ok(Self {
            url,
            given: None,
            expanded: false,
            force_relative: false,
        })
    }
}

impl<P: SerializationPolicy> VerbatimUrl<P> {
    /// Create a [`VerbatimUrl`] from a [`SafeUrl`].
    pub fn from_url(url: SafeUrl<P>) -> Self {
        Self {
            url,
            given: None,
            expanded: false,
            force_relative: false,
        }
    }

    /// Change the serialization policy while retaining the original input and path preferences.
    #[must_use]
    pub fn with_policy<Q: SerializationPolicy>(self, policy: Q) -> VerbatimUrl<Q> {
        VerbatimUrl {
            url: self.url.with_policy(policy),
            given: self.given,
            expanded: self.expanded,
            force_relative: self.force_relative,
        }
    }

    /// Set a fragment that was provided as part of a URL.
    #[must_use]
    #[cfg(feature = "non-pep508-extensions")]
    fn with_url_fragment(mut self, fragment: Option<&str>) -> Self {
        if let Some(fragment) = fragment {
            self.url.set_fragment(Some(fragment));
        }
        self
    }

    /// Set the verbatim representation of the URL.
    #[must_use]
    pub fn with_given(self, given: impl AsRef<str>) -> Self {
        Self {
            given: Some(ArcStr::from(given.as_ref())),
            ..self
        }
    }

    /// Set whether this URL should be represented by a relative path regardless of its input.
    ///
    /// When `false`, preserve the original input's path preference.
    #[must_use]
    pub fn with_force_relative(self, force_relative: bool) -> Self {
        Self {
            force_relative,
            ..self
        }
    }

    /// Return whether this URL is forced to prefer a relative path.
    pub fn force_relative(&self) -> bool {
        self.force_relative
    }

    /// Return the original string as given by the user, if available.
    pub fn given(&self) -> Option<&str> {
        self.given.as_deref()
    }

    /// Return whether this URL should be represented by a relative path.
    ///
    /// If the URL was a PEP 508 URL which contained environment variable references which were
    /// expanded, this function returns true to preserve existing usecases which may rely on
    /// things like `${PWD}` or `${PROJECT_ROOT}`.
    pub fn prefers_relative(&self) -> bool {
        let Some(given) = &self.given else {
            return true;
        };
        if self.expanded || self.force_relative {
            return true;
        }

        if let Some((scheme, _)) = split_scheme(given)
            && let Some(parsed_scheme) = Scheme::parse(scheme)
        {
            return !parsed_scheme.is_file();
        }

        !Path::new(given.as_str()).is_absolute()
    }

    /// Set the "given value contained variables which were expanded" flag.
    ///
    /// Intended to only be used by the URL parser implementations.
    #[must_use]
    pub(crate) fn with_expanded(self, expanded: bool) -> Self {
        Self { expanded, ..self }
    }

    /// Return the underlying [`SafeUrl`].
    pub fn raw(&self) -> &SafeUrl<P> {
        &self.url
    }

    /// Convert a [`VerbatimUrl`] into a [`SafeUrl`].
    pub fn to_url(&self) -> SafeUrl<P> {
        self.url.clone()
    }

    /// Convert the [`VerbatimUrl`] into a [`SafeUrl`].
    ///
    /// To pass the result to an API that requires [`url::Url`], call [`SafeUrl::into_unsafe_url`].
    /// Direct conversion to [`url::Url`] using [`From`] is not supported:
    ///
    /// ```compile_fail
    /// use uv_pep508::VerbatimUrl;
    ///
    /// let url = VerbatimUrl::parse_url("https://example.com")?;
    /// let _: url::Url = url.into();
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn into_url(self) -> SafeUrl<P> {
        self.url
    }

    /// Return the underlying [`Path`], if the URL is a file URL.
    #[cfg(feature = "non-pep508-extensions")]
    pub fn as_path(&self) -> Result<PathBuf, VerbatimUrlError> {
        self.url
            .to_file_path()
            .map_err(|()| VerbatimUrlError::PathConversion(Box::new(self.url.as_url().clone())))
    }
}

impl<P: SerializationPolicy> Ord for VerbatimUrl<P> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.url.cmp(&other.url)
    }
}

impl<P: SerializationPolicy> PartialOrd for VerbatimUrl<P> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl std::str::FromStr for VerbatimUrl {
    type Err = VerbatimUrlError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse_url(s).map(|url| url.with_given(s))
    }
}

impl<P: SerializationPolicy> std::fmt::Display for VerbatimUrl<P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.url, f)
    }
}

impl<P: SerializationPolicy> Deref for VerbatimUrl<P> {
    type Target = SafeUrl<P>;

    fn deref(&self) -> &Self::Target {
        &self.url
    }
}

#[cfg(feature = "unsafe-url")]
#[expect(clippy::disallowed_types, reason = "Supports the opt-in raw URL API")]
impl From<Url> for VerbatimUrl {
    fn from(url: Url) -> Self {
        Self::from_url(SafeUrl::from_url(url))
    }
}

impl<P: SerializationPolicy> From<SafeUrl<P>> for VerbatimUrl<P> {
    fn from(url: SafeUrl<P>) -> Self {
        Self::from_url(url)
    }
}

#[cfg(feature = "serde")]
impl<'de, P: SerializationPolicy> serde::Deserialize<'de> for VerbatimUrl<P> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let url = SafeUrl::<P>::deserialize(deserializer)?;
        Ok(Self::from_url(url))
    }
}

#[cfg(feature = "serde")]
impl<P: SerializationPolicy> serde::Serialize for VerbatimUrl<P>
where
    SafeUrl<P>: serde::Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.url.serialize(serializer)
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

        let vars_expanded = were_vars_expanded(url, expanded.as_ref());

        if let Some((scheme, path)) = split_scheme(&expanded) {
            match Scheme::parse(scheme) {
                // Ex) `file:///home/ferris/project/scripts/...`, `file://localhost/home/ferris/project/scripts/...`, or `file:../ferris/`
                Some(Scheme::File) => {
                    // Strip the leading slashes, along with the `localhost` host, if present.
                    cfg_select! {
                        feature = "non-pep508-extensions" => {
                            // Transform, e.g., `/C:/Users/ferris/wheel-0.42.0.tar.gz` to `C:\Users\ferris\wheel-0.42.0.tar.gz`.
                            let (path, fragment) = path
                                .split_once('#')
                                .map_or((path, None), |(path, fragment)| (path, Some(fragment)));
                            let path = strip_host(path);

                            let path = normalize_url_path(path);

                            if let Some(working_dir) = working_dir {
                                return Ok(Self::from_path(path.as_ref(), working_dir)?
                                    .with_url_fragment(fragment)
                                    .with_given(url)
                                    .with_expanded(vars_expanded));
                            }

                            Ok(Self::from_absolute_path(path.as_ref())?
                                .with_url_fragment(fragment)
                                .with_given(url)
                                .with_expanded(vars_expanded))
                        },
                        _ => {
                            Ok(Self::parse_url(expanded)?
                                .with_given(url)
                                .with_expanded(vars_expanded))
                        },
                    }
                }

                // Ex) `https://download.pytorch.org/whl/torch_stable.html`
                Some(_) => {
                    // Ex) `https://download.pytorch.org/whl/torch_stable.html`
                    Ok(Self::parse_url(expanded.as_ref())?.with_given(url))
                }

                // Ex) `C:\Users\ferris\wheel-0.42.0.tar.gz`
                _ => {
                    cfg_select! {
                        feature = "non-pep508-extensions" => {
                            Ok(
                                Self::from_path_with_fragment(expanded.as_ref(), working_dir)?
                                    .with_given(url)
                                    .with_expanded(vars_expanded),
                            )
                        },
                        _ => Err(Self::Err::NotAUrl(expanded.to_string())),
                    }
                }
            }
        } else {
            // Ex) `../editable/`
            cfg_select! {
                feature = "non-pep508-extensions" => {
                    Ok(
                        Self::from_path_with_fragment(expanded.as_ref(), working_dir)?
                            .with_given(url)
                            .with_expanded(vars_expanded),
                    )
                },
                _ => Err(Self::Err::NotAUrl(expanded.to_string())),
            }
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
    Url(#[from] SafeUrlError),

    /// Received a relative path, but no working directory was provided.
    #[error("relative path without a working directory: {0}")]
    WorkingDirectory(PathBuf),

    /// Received a path that could not be converted to a URL.
    #[error("path could not be converted to a URL: {0}")]
    UrlConversion(PathBuf),

    /// Received a URL that could not be converted to a path.
    #[error("URL could not be converted to a path: {0}")]
    PathConversion(Box<SafeUrl>),

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

    let mut expanded = None;
    let mut previous_end = 0;
    for (start, _) in s.match_indices("${") {
        let remaining = &s[start + 2..];
        let Some(length) = remaining
            .bytes()
            .position(|byte| !byte.is_ascii_uppercase() && !byte.is_ascii_digit() && byte != b'_')
        else {
            continue;
        };
        if length == 0 || remaining.as_bytes()[length] != b'}' {
            continue;
        }

        let name = &remaining[..length];
        let end = start + 2 + length + 1;
        let expanded = expanded.get_or_insert_with(|| String::with_capacity(s.len()));
        expanded.push_str(&s[previous_end..start]);
        match std::env::var(name) {
            Ok(value) => expanded.push_str(&value),
            Err(_) if name == "PROJECT_ROOT" => expanded.push_str(&PROJECT_ROOT_FRAGMENT),
            Err(_) => expanded.push_str(&s[start..end]),
        }
        previous_end = end;
    }

    if let Some(mut expanded) = expanded {
        expanded.push_str(&s[previous_end..]);
        Cow::Owned(expanded)
    } else {
        Cow::Borrowed(s)
    }
}

/// Returns `true` if [`expand_env_vars`] changed the given value.
///
/// Note: If a variable named `FOO` expands to `${FOO}` then this will produce a false negative.
/// This seems like too much of a corner case to justify trying to fix it.
pub(crate) fn were_vars_expanded(given: &str, expanded: &str) -> bool {
    expanded != given
}

/// Like [`url::Url::parse`], but only splits the scheme. Derived from the `url` crate.
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
#[cfg(feature = "non-pep508-extensions")]
pub(crate) fn strip_host(path: &str) -> &str {
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
pub fn looks_like_git_repository<P: SerializationPolicy>(url: &SafeUrl<P>) -> bool {
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
#[cfg(feature = "non-pep508-extensions")]
fn split_fragment(path: &Path) -> (Cow<'_, Path>, Option<&str>) {
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
    fn is_file(&self) -> bool {
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
    use insta::assert_snapshot;
    #[cfg(feature = "serde")]
    use uv_redacted::policy::{Raw, Stripped};

    use super::*;

    #[test]
    #[cfg(feature = "unsafe-url")]
    #[expect(clippy::disallowed_types, reason = "Exercises the opt-in raw URL API")]
    fn from_raw_url() {
        let raw = Url::parse("https://user:password@EXAMPLE.COM/demo.whl?sig=signature").unwrap();
        let url = VerbatimUrl::from(raw);
        assert_eq!(
            url.raw().as_str(),
            "https://user:password@example.com/demo.whl?sig=signature"
        );
        assert_eq!(url.given(), None);
        assert_snapshot!(url, @"https://user:****@example.com/demo.whl?sig=****");
    }

    #[test]
    #[cfg(feature = "serde")]
    fn serialization_policy_retains_verbatim_input() -> Result<(), Box<dyn std::error::Error>> {
        let input = "https://user:password@EXAMPLE.COM/file.whl?sig=abc%2Bdef%3D";
        let url = VerbatimUrl::parse_url(input)?
            .with_given(input)
            .with_force_relative(true)
            .with_policy(Stripped);
        assert_eq!(url.given(), Some(input));
        assert!(url.force_relative());
        assert_eq!(url.password(), Some("password"));
        assert_snapshot!(serde_json::to_string(&url)?, @r#""https://example.com/file.whl?sig=abc%2Bdef%3D""#);

        let url = url.with_policy(Raw);
        assert_eq!(url.given(), Some(input));
        assert!(url.force_relative());
        assert_snapshot!(serde_json::to_string(&url)?, @r#""https://user:password@example.com/file.whl?sig=abc%2Bdef%3D""#);
        let roundtrip: VerbatimUrl<Raw> = serde_json::from_str(&serde_json::to_string(&url)?)?;
        assert_eq!(roundtrip.raw(), url.raw());
        assert_eq!(roundtrip.given(), None);
        Ok(())
    }

    #[test]
    #[cfg(feature = "non-pep508-extensions")]
    fn as_path_non_file_url() -> Result<(), VerbatimUrlError> {
        let url: VerbatimUrl = "https://user:password@example.com".parse()?;
        assert_snapshot!(url.as_path().expect_err("HTTPS URL is not a file path"), @"URL could not be converted to a path: https://user:****@example.com/");

        Ok(())
    }

    #[test]
    #[cfg(all(windows, feature = "non-pep508-extensions"))]
    fn as_path_localhost_unc() -> Result<(), VerbatimUrlError> {
        let url = VerbatimUrl::from_absolute_path(r"\\localhost\share\package")?;
        assert_snapshot!(url.as_path().expect_err("localhost URL has no drive letter"), @"URL could not be converted to a path: file://localhost/share/package");

        Ok(())
    }

    #[test]
    fn forced_relative_overrides_absolute_spelling() -> Result<(), VerbatimUrlError> {
        let absolute: VerbatimUrl = "file:///path/to/distribution".parse()?;
        let relative = absolute.clone().with_force_relative(true);

        assert!(relative.prefers_relative());
        assert_eq!(absolute, relative);
        assert!(!relative.with_force_relative(false).prefers_relative());

        Ok(())
    }

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
    #[cfg(feature = "non-pep508-extensions")]
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
    fn hash_in_path() {
        let assert_path = |url: VerbatimUrl, path: &Path| {
            assert_eq!(url.fragment(), None);
            assert_eq!(url.to_file_path().unwrap(), path);
            assert!(url.as_str().ends_with("scripts%23hash=somehash"));
        };

        let path = std::path::absolute("scripts#hash=somehash").unwrap();
        assert_path(VerbatimUrl::from_absolute_path(&path).unwrap(), &path);
        assert_path(VerbatimUrl::from_normalized_path(&path).unwrap(), &path);

        #[cfg(feature = "non-pep508-extensions")]
        {
            let base_dir = std::env::current_dir().unwrap();
            let path = base_dir.join("scripts#hash=somehash");
            assert_path(
                VerbatimUrl::from_path("scripts#hash=somehash", base_dir).unwrap(),
                &path,
            );
        }
    }

    #[test]
    fn git_repository() {
        let url = SafeUrl::parse("https://github.com/user/repo.git").unwrap();
        assert!(looks_like_git_repository(&url));

        let url = SafeUrl::parse("https://gitlab.com/user/repo.git").unwrap();
        assert!(looks_like_git_repository(&url));

        let url = SafeUrl::parse("https://bitbucket.org/user/repo.git").unwrap();
        assert!(looks_like_git_repository(&url));

        let url = SafeUrl::parse("https://github.com/user/repo").unwrap();
        assert!(looks_like_git_repository(&url));

        let url = SafeUrl::parse("https://example.com/user/repo.git").unwrap();
        assert!(!looks_like_git_repository(&url));

        let url = SafeUrl::parse("https://github.com/user").unwrap();
        assert!(!looks_like_git_repository(&url));

        let url = SafeUrl::parse("https://github.com/user/repo.zip").unwrap();
        assert!(!looks_like_git_repository(&url));

        let url = SafeUrl::parse("https://github.com/").unwrap();
        assert!(!looks_like_git_repository(&url));

        let url = SafeUrl::parse_relaxed("").unwrap_err();
        assert_eq!(url.to_string(), "relative URL without a base");

        let url = SafeUrl::parse_relaxed("github.com/user/repo.git").unwrap_err();
        assert_eq!(url.to_string(), "relative URL without a base");

        let url = SafeUrl::parse("https://github.com/user/repo/extra.git").unwrap();
        assert!(!looks_like_git_repository(&url));

        let url = SafeUrl::parse("https://github.com/user/repo.GIT").unwrap();
        assert!(looks_like_git_repository(&url));

        let url = SafeUrl::parse("https://github.com/user/repo.git?foo=bar").unwrap();
        assert!(looks_like_git_repository(&url));

        let url = SafeUrl::parse("https://github.com/user/repo.git#readme").unwrap();
        assert!(looks_like_git_repository(&url));

        let url = SafeUrl::parse("https://github.com/pypa/pip/archive/1.3.1.zip#sha1=da9234ee9982d4bbb3c72346a6de940a148ea686").unwrap();
        assert!(!looks_like_git_repository(&url));
    }

    #[test]
    fn parse_url_ambiguous() {
        assert_snapshot!(
            VerbatimUrl::parse_url("https://user/name:password@domain/a/b/c").unwrap_err().to_string(),
            @"ambiguous user/pass authority in URL (not percent-encoded?): https:***@domain/a/b/c"
        );

        assert_snapshot!(
            VerbatimUrl::parse_url("https://user\\name:password@domain/a/b/c").unwrap_err().to_string(),
            @"ambiguous user/pass authority in URL (not percent-encoded?): https:***@domain/a/b/c"
        );

        assert_snapshot!(
            VerbatimUrl::parse_url("https://user#name:password@domain/a/b/c").unwrap_err().to_string(),
            @"ambiguous user/pass authority in URL (not percent-encoded?): https:***@domain/a/b/c"
        );

        assert_snapshot!(
            VerbatimUrl::parse_url("https://user.com/name:password@domain/a/b/c").unwrap_err().to_string(),
            @"ambiguous user/pass authority in URL (not percent-encoded?): https:***@domain/a/b/c"
        );
    }

    #[test]
    fn env_vars() {
        temp_env::with_vars(
            [
                ("FOO", None),
                ("BAR", Some("bar")),
                ("BAZ", Some("baz")),
                ("Not-Valid", Some("Not-Valid")),
                ("TEST_1", Some("Test 1")),
                ("1", Some("one")),
                ("EMPTY", Some("")),
                ("LITERAL", Some("${BAR}")),
                ("PROJECT_ROOT", None),
            ],
            || {
                // Basic
                assert_eq!(expand_env_vars(""), Cow::Borrowed(""));
                assert_eq!(expand_env_vars("test"), Cow::Borrowed("test"));
                assert_eq!(expand_env_vars("$"), Cow::Borrowed("$"));

                // Invalid
                assert_eq!(expand_env_vars("$FOO"), Cow::Borrowed("$FOO"));
                assert_eq!(expand_env_vars("$BAR"), Cow::Borrowed("$BAR"));
                assert_eq!(expand_env_vars("${BAR"), Cow::Borrowed("${BAR"));
                assert_eq!(expand_env_vars("$BAR}"), Cow::Borrowed("$BAR}"));
                assert_eq!(expand_env_vars("${ BAR }"), Cow::Borrowed("${ BAR }"));
                assert_eq!(
                    expand_env_vars("${Not-Valid}"),
                    Cow::Borrowed("${Not-Valid}")
                );
                assert_eq!(expand_env_vars("${}"), Cow::Borrowed("${}"));
                assert_eq!(expand_env_vars("${BÁR}"), Cow::Borrowed("${BÁR}"));

                // Missing
                assert_eq!(expand_env_vars("${FOO}"), "${FOO}");

                // Case sensitive
                assert_eq!(expand_env_vars("${bar}"), "${bar}");

                // One variable referenced
                assert_eq!(expand_env_vars("${BAR}"), "bar");
                assert_eq!(expand_env_vars("foo ${BAR}"), "foo bar");
                assert_eq!(expand_env_vars("foo ${BAR} baz"), "foo bar baz");
                assert_eq!(expand_env_vars("foo ${BAR} baz ${BAR}"), "foo bar baz bar");

                // Two variables referenced
                assert_eq!(expand_env_vars("${FOO} ${BAR} ${BAZ}"), "${FOO} bar baz");
                assert_eq!(expand_env_vars("<${BAR}-${BAZ}>"), "<bar-baz>");
                assert_eq!(
                    expand_env_vars("${FOO}${BAR}${BAZ}${FOO}${BAR}${BAZ}"),
                    "${FOO}barbaz${FOO}barbaz"
                );

                // Weird
                assert_eq!(expand_env_vars("${${TEST_1}}"), "${Test 1}");
                assert_eq!(expand_env_vars("${INVALID-${BAR}}"), "${INVALID-bar}");
                assert_eq!(expand_env_vars("é${BAR}🦀${BAZ}"), "ébar🦀baz");
                assert_eq!(expand_env_vars("${BAR}${BAZ"), "bar${BAZ");
                assert_eq!(expand_env_vars("${1}"), "one");
                assert_eq!(expand_env_vars("${EMPTY}"), "");
                assert_eq!(expand_env_vars("${LITERAL}"), "${BAR}");

                // PROJECT_ROOT
                let cwd = std::env::current_dir().unwrap();
                let cwd = cwd.to_string_lossy();

                assert_eq!(
                    expand_env_vars("${PROJECT_ROOT}/file"),
                    format!("{cwd}/file")
                );
                assert_eq!(
                    expand_env_vars("$PROJECT_ROOT/file.txt"),
                    Cow::Borrowed("$PROJECT_ROOT/file.txt")
                );

                assert_eq!(
                    expand_env_vars("${FOO} ${BAR} ${PROJECT_ROOT} ${BAZ}"),
                    format!("${{FOO}} bar {cwd} baz")
                );
            },
        );
    }
}
