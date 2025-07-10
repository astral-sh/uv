use serde::{Deserialize, Serialize};
use uv_redacted::DisplaySafeUrl;

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
