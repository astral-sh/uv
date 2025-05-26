use serde::{Deserialize, Serialize};
use uv_redacted::DisplaySafeUrl;

/// Join a relative URL to a base URL.
pub fn base_url_join_relative(
    base: &str,
    relative: &str,
) -> Result<DisplaySafeUrl, JoinRelativeError> {
    let base_url = DisplaySafeUrl::parse(base).map_err(|err| JoinRelativeError::ParseError {
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
        serialize_with = "DisplaySafeUrl::serialize_internal",
        deserialize_with = "DisplaySafeUrl::deserialize_internal"
    )]
    DisplaySafeUrl,
);

impl BaseUrl {
    /// Return the underlying [`DisplaySafeUrl`].
    pub fn as_url(&self) -> &DisplaySafeUrl {
        &self.0
    }

    /// Return the underlying [`DisplaySafeUrl`] as a serialized string.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<DisplaySafeUrl> for BaseUrl {
    fn from(url: DisplaySafeUrl) -> Self {
        Self(url)
    }
}

impl std::fmt::Display for BaseUrl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
