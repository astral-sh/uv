use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct BaseUrl(Url);

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
