use serde::{Deserialize, Serialize};
use std::str::FromStr;
use thiserror::Error;
use url::Url;
use uv_redacted::{DisplaySafeUrl, DisplaySafeUrlError};

#[derive(Error, Debug)]
pub enum ServiceParseError {
    #[error(transparent)]
    InvalidUrl(#[from] DisplaySafeUrlError),
    #[error("Unsupported scheme: {0}")]
    UnsupportedScheme(String),
    #[error("HTTPS is required for non-local hosts")]
    HttpsRequired,
}

/// A service URL that wraps [`DisplaySafeUrl`] for CLI usage.
///
/// This type provides automatic URL parsing and validation when used as a CLI argument,
/// eliminating the need for manual parsing in command functions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(transparent)]
pub struct Service(DisplaySafeUrl);

impl Service {
    /// Get the underlying [`DisplaySafeUrl`].
    pub fn url(&self) -> &DisplaySafeUrl {
        &self.0
    }

    /// Convert into the underlying [`DisplaySafeUrl`].
    pub fn into_url(self) -> DisplaySafeUrl {
        self.0
    }

    /// Validate that the URL scheme is supported.
    fn check_scheme(url: &Url) -> Result<(), ServiceParseError> {
        match url.scheme() {
            "https" => Ok(()),
            "http" if matches!(url.host_str(), Some("localhost" | "127.0.0.1")) => Ok(()),
            "http" => Err(ServiceParseError::HttpsRequired),
            value => Err(ServiceParseError::UnsupportedScheme(value.to_string())),
        }
    }
}

impl FromStr for Service {
    type Err = ServiceParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // First try parsing as-is
        let url = match DisplaySafeUrl::parse(s) {
            Ok(url) => url,
            Err(DisplaySafeUrlError::Url(url::ParseError::RelativeUrlWithoutBase)) => {
                // If it's a relative URL, try prepending https://
                let with_https = format!("https://{s}");
                DisplaySafeUrl::parse(&with_https)?
            }
            Err(err) => return Err(err.into()),
        };

        Self::check_scheme(&url)?;

        Ok(Self(url))
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

impl TryFrom<DisplaySafeUrl> for Service {
    type Error = ServiceParseError;

    fn try_from(value: DisplaySafeUrl) -> Result<Self, Self::Error> {
        Self::check_scheme(&value)?;
        Ok(Self(value))
    }
}
