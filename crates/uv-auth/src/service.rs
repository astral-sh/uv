use serde::{Deserialize, Serialize};
use std::str::FromStr;
use thiserror::Error;
use uv_redacted::policy::Raw;
use uv_redacted::{SafeUrl, SafeUrlError};

#[derive(Error, Debug)]
pub enum ServiceParseError {
    #[error(transparent)]
    InvalidUrl(#[from] SafeUrlError),
    #[error("Unsupported scheme: {0}")]
    UnsupportedScheme(String),
    #[error("HTTPS is required for non-local hosts")]
    HttpsRequired,
}

/// A service URL that wraps [`SafeUrl`] for CLI usage.
///
/// This type provides automatic URL parsing and validation when used as a CLI argument,
/// eliminating the need for manual parsing in command functions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(transparent)]
pub struct Service(SafeUrl<Raw>);

impl Service {
    /// Get the underlying [`SafeUrl`].
    pub fn url(&self) -> &SafeUrl {
        self.0.as_url()
    }

    /// Validate that the URL scheme is supported.
    fn check_scheme(url: &SafeUrl) -> Result<(), ServiceParseError> {
        match url.scheme() {
            "https" => Ok(()),
            "http" if matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]")) => Ok(()),
            "http" => Err(ServiceParseError::HttpsRequired),
            value => Err(ServiceParseError::UnsupportedScheme(value.to_string())),
        }
    }
}

impl FromStr for Service {
    type Err = ServiceParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // First try parsing as-is
        let url = match SafeUrl::parse(s) {
            Ok(url) => url,
            Err(SafeUrlError::Url(url::ParseError::RelativeUrlWithoutBase)) => {
                // If it's a relative URL, try prepending https://
                let with_https = format!("https://{s}");
                SafeUrl::parse(&with_https)?
            }
            Err(err) => return Err(err.into()),
        };

        Self::check_scheme(&url)?;

        Ok(Self(url.with_policy(Raw)))
    }
}

impl std::fmt::Display for Service {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl TryFrom<String> for Service {
    type Error = ServiceParseError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::from_str(&value)
    }
}

impl From<Service> for String {
    fn from(service: Service) -> Self {
        service.to_string()
    }
}

impl TryFrom<SafeUrl> for Service {
    type Error = ServiceParseError;

    fn try_from(value: SafeUrl) -> Result<Self, Self::Error> {
        Self::check_scheme(&value)?;
        Ok(Self(value.with_policy(Raw)))
    }
}
