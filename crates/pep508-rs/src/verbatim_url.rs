use std::borrow::Cow;
use std::fmt::Debug;
use std::ops::Deref;
use std::path::{Component, Path, PathBuf};

use once_cell::sync::Lazy;
use regex::Regex;
use url::Url;

/// A wrapper around [`Url`] that preserves the original string.
#[derive(Debug, Clone, Eq, derivative::Derivative)]
#[derivative(PartialEq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct VerbatimUrl {
    /// The parsed URL.
    #[cfg_attr(
        feature = "serde",
        serde(
            serialize_with = "Url::serialize_internal",
            deserialize_with = "Url::deserialize_internal"
        )
    )]
    url: Url,
    /// The URL as it was provided by the user.
    #[derivative(PartialEq = "ignore")]
    #[derivative(Hash = "ignore")]
    given: Option<String>,
}

impl VerbatimUrl {
    /// Parse a URL from a string, expanding any environment variables.
    pub fn parse(given: impl AsRef<str>) -> Result<Self, VerbatimUrlError> {
        let url = Url::parse(&expand_env_vars(given.as_ref(), true))
            .map_err(|err| VerbatimUrlError::Url(given.as_ref().to_owned(), err))?;
        Ok(Self { url, given: None })
    }

    /// Parse a URL from am absolute or relative path.
    pub fn from_path(path: impl AsRef<str>, working_dir: impl AsRef<Path>) -> Self {
        // Expand any environment variables.
        let path = PathBuf::from(expand_env_vars(path.as_ref(), false).as_ref());

        // Convert the path to an absolute path, if necessary.
        let path = if path.is_absolute() {
            path
        } else {
            working_dir.as_ref().join(path)
        };

        // Normalize the path.
        let path = normalize_path(&path);

        // Convert to a URL.
        let url = Url::from_file_path(path).expect("path is absolute");

        Self { url, given: None }
    }

    /// Parse a URL from an absolute path.
    pub fn from_absolute_path(path: impl AsRef<str>) -> Result<Self, VerbatimUrlError> {
        // Expand any environment variables.
        let path = PathBuf::from(expand_env_vars(path.as_ref(), false).as_ref());

        // Convert the path to an absolute path, if necessary.
        let path = if path.is_absolute() {
            path
        } else {
            return Err(VerbatimUrlError::RelativePath(path));
        };

        // Normalize the path.
        let path = normalize_path(&path);

        // Convert to a URL.
        let url = Url::from_file_path(path).expect("path is absolute");

        Ok(Self { url, given: None })
    }

    /// Set the verbatim representation of the URL.
    #[must_use]
    pub fn with_given(self, given: String) -> Self {
        Self {
            given: Some(given),
            ..self
        }
    }

    /// Return the original string as given by the user, if available.
    pub fn given(&self) -> Option<&str> {
        self.given.as_deref()
    }

    /// Return the underlying [`Url`].
    pub fn raw(&self) -> &Url {
        &self.url
    }

    /// Convert a [`VerbatimUrl`] into a [`Url`].
    pub fn to_url(&self) -> Url {
        self.url.clone()
    }

    /// Create a [`VerbatimUrl`] from a [`Url`].
    ///
    /// This method should be used sparingly (ideally, not at all), as it represents a loss of the
    /// verbatim representation.
    pub fn unknown(url: Url) -> Self {
        Self { given: None, url }
    }
}

impl std::str::FromStr for VerbatimUrl {
    type Err = VerbatimUrlError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s).map(|url| url.with_given(s.to_owned()))
    }
}

impl std::fmt::Display for VerbatimUrl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.url, f)
    }
}

impl Deref for VerbatimUrl {
    type Target = Url;

    fn deref(&self) -> &Self::Target {
        &self.url
    }
}

/// An error that can occur when parsing a [`VerbatimUrl`].
#[derive(thiserror::Error, Debug)]
pub enum VerbatimUrlError {
    /// Failed to parse a URL.
    #[error("{0}")]
    Url(String, #[source] url::ParseError),

    /// Received a relative path, but no working directory was provided.
    #[error("relative path without a working directory: {0}")]
    RelativePath(PathBuf),
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
fn expand_env_vars(s: &str, escape: bool) -> Cow<'_, str> {
    // Generate the project root, to be used via the `${PROJECT_ROOT}`
    // environment variable.
    static PROJECT_ROOT_FRAGMENT: Lazy<String> = Lazy::new(|| {
        let project_root = std::env::current_dir().unwrap();
        project_root.to_string_lossy().to_string()
    });

    static RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"(?P<var>\$\{(?P<name>[A-Z0-9_]+)})").unwrap());

    RE.replace_all(s, |caps: &regex::Captures<'_>| {
        let name = caps.name("name").unwrap().as_str();
        std::env::var(name).unwrap_or_else(|_| match name {
            // Ensure that the variable is URL-escaped, if necessary.
            "PROJECT_ROOT" => {
                if escape {
                    PROJECT_ROOT_FRAGMENT.replace(' ', "%20")
                } else {
                    PROJECT_ROOT_FRAGMENT.to_string()
                }
            }
            _ => caps["var"].to_owned(),
        })
    })
}

/// Normalize a path, removing things like `.` and `..`.
///
/// Source: <https://github.com/rust-lang/cargo/blob/b48c41aedbd69ee3990d62a0e2006edbb506a480/crates/cargo-util/src/paths.rs#L76C1-L109C2>
fn normalize_path(path: &Path) -> PathBuf {
    let mut components = path.components().peekable();
    let mut ret = if let Some(c @ Component::Prefix(..)) = components.peek().copied() {
        components.next();
        PathBuf::from(c.as_os_str())
    } else {
        PathBuf::new()
    };

    for component in components {
        match component {
            Component::Prefix(..) => unreachable!(),
            Component::RootDir => {
                ret.push(component.as_os_str());
            }
            Component::CurDir => {}
            Component::ParentDir => {
                ret.pop();
            }
            Component::Normal(c) => {
                ret.push(c);
            }
        }
    }
    ret
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
}
