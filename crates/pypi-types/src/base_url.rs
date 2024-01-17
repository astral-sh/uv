use serde::{Deserialize, Serialize};
use url::Url;

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
