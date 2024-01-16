use std::borrow::Cow;
use std::ops::Deref;
use std::path::Path;

use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use url::Url;

/// A wrapper around [`Url`] that preserves the original string.
#[derive(Debug, Clone, Eq, derivative::Derivative)]
#[derivative(PartialEq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct VerbatimUrl {
    /// The parsed URL.
    #[serde(
        serialize_with = "Url::serialize_internal",
        deserialize_with = "Url::deserialize_internal"
    )]
    url: Url,
    /// The URL as it was provided by the user.
    #[derivative(PartialEq = "ignore")]
    #[derivative(Hash = "ignore")]
    given: Option<String>,
}

impl VerbatimUrl {
    /// Parse a URL from a string, expanding any environment variables.
    pub fn parse(given: String) -> Result<Self, Error> {
        let url = Url::parse(&expand_env_vars(&given))?;
        Ok(Self {
            given: Some(given),
            url,
        })
    }

    /// Parse a URL from a path.
    #[allow(clippy::result_unit_err)]
    pub fn from_path(path: impl AsRef<Path>, given: String) -> Result<Self, ()> {
        Ok(Self {
            url: Url::from_directory_path(path)?,
            given: Some(given),
        })
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
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s.to_owned())
    }
}

impl std::fmt::Display for VerbatimUrl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.url.fmt(f)
    }
}

impl Deref for VerbatimUrl {
    type Target = Url;

    fn deref(&self) -> &Self::Target {
        &self.url
    }
}

#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum Error {
    #[error(transparent)]
    Url(#[from] url::ParseError),
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
fn expand_env_vars(s: &str) -> Cow<'_, str> {
    // Generate a URL-escaped project root, to be used via the `${PROJECT_ROOT}`
    // environment variable. Ensure that it's URL-escaped.
    static PROJECT_ROOT_FRAGMENT: Lazy<String> = Lazy::new(|| {
        let project_root = std::env::current_dir().unwrap();
        project_root.to_string_lossy().replace(' ', "%20")
    });

    static RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"(?P<var>\$\{(?P<name>[A-Z0-9_]+)})").unwrap());

    RE.replace_all(s, |caps: &regex::Captures<'_>| {
        let name = caps.name("name").unwrap().as_str();
        std::env::var(name).unwrap_or_else(|_| match name {
            "PROJECT_ROOT" => PROJECT_ROOT_FRAGMENT.clone(),
            _ => caps["var"].to_owned(),
        })
    })
}
