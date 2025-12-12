#[cfg(feature = "schemars")]
use std::borrow::Cow;
use std::fmt::{self, Display, Formatter};
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize};
use url::Url;

/// A validated proxy URL.
///
/// This type validates that the URL is a valid proxy URL on construction,
/// ensuring that it has a valid scheme (http, https, socks5, or socks5h).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProxyUrl(Url);

impl ProxyUrl {
    /// Returns the underlying URL as a string.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Returns a reference to the underlying [`Url`].
    pub fn as_url(&self) -> &Url {
        &self.0
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ProxyUrlError {
    #[error("invalid proxy URL: {0}")]
    InvalidUrl(#[from] url::ParseError),
    #[error("proxy URL must have a scheme (e.g., http://, https://, socks5://): `{0}`")]
    MissingScheme(String),
    #[error(
        "invalid proxy URL scheme `{scheme}` in `{url}`: expected http, https, socks5, or socks5h"
    )]
    InvalidScheme { scheme: String, url: String },
}

impl FromStr for ProxyUrl {
    type Err = ProxyUrlError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let url = Url::parse(s)?;

        // Validate the scheme
        let scheme = url.scheme();
        if scheme.is_empty() {
            return Err(ProxyUrlError::MissingScheme(s.to_string()));
        }

        // reqwest supports http, https, socks5, and socks5h schemes for proxies
        match scheme {
            "http" | "https" | "socks5" | "socks5h" => {}
            _ => {
                return Err(ProxyUrlError::InvalidScheme {
                    scheme: scheme.to_string(),
                    url: s.to_string(),
                });
            }
        }

        Ok(Self(url))
    }
}

impl Display for ProxyUrl {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        Display::fmt(&self.0, f)
    }
}

impl<'de> Deserialize<'de> for ProxyUrl {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::from_str(&s).map_err(serde::de::Error::custom)
    }
}

impl Serialize for ProxyUrl {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

#[cfg(feature = "schemars")]
impl schemars::JsonSchema for ProxyUrl {
    fn schema_name() -> Cow<'static, str> {
        Cow::Borrowed("ProxyUrl")
    }

    fn json_schema(_generator: &mut schemars::generate::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "format": "uri",
            "description": "A proxy URL (e.g., `http://proxy.example.com:8080`)."
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_valid_proxy_urls() {
        // HTTP proxy
        let url = "http://proxy.example.com:8080".parse::<ProxyUrl>().unwrap();
        assert!(url.as_str().starts_with("http://proxy.example.com:8080"));

        // HTTPS proxy
        let url = "https://proxy.example.com:8080"
            .parse::<ProxyUrl>()
            .unwrap();
        assert!(url.as_str().starts_with("https://proxy.example.com:8080"));

        // SOCKS5 proxy
        let url = "socks5://proxy.example.com:1080"
            .parse::<ProxyUrl>()
            .unwrap();
        assert!(url.as_str().starts_with("socks5://proxy.example.com:1080"));

        // SOCKS5H proxy
        let url = "socks5h://proxy.example.com:1080"
            .parse::<ProxyUrl>()
            .unwrap();
        assert!(url.as_str().starts_with("socks5h://proxy.example.com:1080"));

        // Proxy with auth
        let url = "http://user:pass@proxy.example.com:8080"
            .parse::<ProxyUrl>()
            .unwrap();
        assert!(
            url.as_str()
                .starts_with("http://user:pass@proxy.example.com:8080")
        );
    }

    #[test]
    fn parse_invalid_proxy_urls() {
        // Missing scheme
        let result = "proxy.example.com:8080".parse::<ProxyUrl>();
        assert!(result.is_err());

        // Invalid scheme
        let result = "ftp://proxy.example.com:8080".parse::<ProxyUrl>();
        assert!(matches!(result, Err(ProxyUrlError::InvalidScheme { .. })));

        // Invalid URL
        let result = "not a url".parse::<ProxyUrl>();
        assert!(result.is_err());
    }
}
