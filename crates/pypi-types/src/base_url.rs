use serde::{Deserialize, Serialize};
use url::Url;

/// Join a relative URL to a base URL.
pub fn base_url_join_relative(base: &str, relative: &str) -> Result<Url, JoinRelativeError> {
    let base_url = Url::parse(base).map_err(|err| JoinRelativeError::ParseError {
        original: base.to_string(),
        source: err,
    })?;

    base_url
        .join(relative)
        .map_err(|err| JoinRelativeError::ParseError {
            original: format!("{base}/{relative}"),
            source: err,
        })
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
