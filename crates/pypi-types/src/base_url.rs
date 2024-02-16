use serde::{Deserialize, Serialize};
use url::Url;

/// Join a possibly relative URL to a base URL.
///
/// When `maybe_relative` is not relative, then it is parsed and returned with
/// `base` being ignored.
///
/// This is useful for parsing URLs that may be absolute or relative, with a
/// known base URL, and that doesn't require having already parsed a `BaseUrl`.
pub fn base_url_join_relative(base: &str, maybe_relative: &str) -> Result<Url, JoinRelativeError> {
    match Url::parse(maybe_relative) {
        Ok(absolute) => Ok(absolute),
        Err(err) => {
            if err == url::ParseError::RelativeUrlWithoutBase {
                let base_url = Url::parse(base).map_err(|err| JoinRelativeError::ParseError {
                    original: base.to_string(),
                    source: err,
                })?;

                base_url
                    .join(maybe_relative)
                    .map_err(|_| JoinRelativeError::ParseError {
                        original: format!("{base}/{maybe_relative}"),
                        source: err,
                    })
            } else {
                Err(JoinRelativeError::ParseError {
                    original: maybe_relative.to_string(),
                    source: err,
                })
            }
        }
    }
}

/// An error that occurs when `base_url_join_relative` fails.
///
/// The error message includes the URL (`base` or `maybe_relative`) passed to
/// `base_url_join_relative` that provoked the error.
#[derive(Clone, Debug, thiserror::Error)]
pub enum JoinRelativeError {
    #[error("Failed to parse URL: `{original}`")]
    ParseError {
        original: String,
        source: url::ParseError,
    },
}

#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct BaseUrl(
    #[serde(
        serialize_with = "Url::serialize_internal",
        deserialize_with = "Url::deserialize_internal"
    )]
    Url,
);

impl BaseUrl {
    /// Parse the given URL. If it's relative, join it to the current [`BaseUrl`]. Allows for
    /// parsing URLs that may be absolute or relative, with a known base URL.
    pub fn join_relative(&self, url: &str) -> Result<Url, url::ParseError> {
        match Url::parse(url) {
            Ok(url) => Ok(url),
            Err(err) => {
                if err == url::ParseError::RelativeUrlWithoutBase {
                    self.0.join(url)
                } else {
                    Err(err)
                }
            }
        }
    }

    /// Return the underlying [`Url`].
    pub fn as_url(&self) -> &Url {
        &self.0
    }
}

impl From<Url> for BaseUrl {
    fn from(url: Url) -> Self {
        Self(url)
    }
}

impl std::fmt::Display for BaseUrl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
