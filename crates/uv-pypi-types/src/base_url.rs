use serde::{Deserialize, Serialize};
use uv_redacted::SafeUrl;

#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct BaseUrl(
    #[serde(
        serialize_with = "SafeUrl::serialize_internal",
        deserialize_with = "SafeUrl::deserialize_internal"
    )]
    SafeUrl,
);

impl BaseUrl {
    /// Return the underlying [`SafeUrl`].
    pub fn as_url(&self) -> &SafeUrl {
        &self.0
    }

    /// Return the underlying [`SafeUrl`] as a serialized string.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<SafeUrl> for BaseUrl {
    fn from(url: SafeUrl) -> Self {
        Self(url)
    }
}

impl std::fmt::Display for BaseUrl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
