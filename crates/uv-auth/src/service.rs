use serde::{Deserialize, Serialize};
use std::str::FromStr;
use thiserror::Error;
use uv_redacted::DisplaySafeUrl;

#[derive(Error, Debug)]
pub enum ServiceParseError {
    #[error("failed to parse URL: {0}")]
    InvalidUrl(#[from] url::ParseError),
    #[error("only HTTPS is supported")]
    UnsupportedScheme,
}

/// A service URL that wraps [`DisplaySafeUrl`] for CLI usage.
///
/// This type provides automatic URL parsing and validation when used as a CLI argument,
/// eliminating the need for manual parsing in command functions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(try_from = "String", into = "String")]
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
}

impl FromStr for Service {
    type Err = ServiceParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // First try parsing as-is
        let url = match DisplaySafeUrl::parse(s) {
            Ok(url) => url,
            Err(url::ParseError::RelativeUrlWithoutBase) => {
                // If it's a relative URL, try prepending https://
                let with_https = format!("https://{s}");
                DisplaySafeUrl::parse(&with_https)?
            }
            Err(err) => return Err(err.into()),
        };

        // Only allow HTTPS URLs (but allow HTTP in tests for convenience)
        #[cfg(not(test))]
        if url.scheme() != "https" {
            return Err(ServiceParseError::UnsupportedScheme);
        }
        #[cfg(test)]
        if url.scheme() != "https" && url.scheme() != "http" {
            return Err(ServiceParseError::UnsupportedScheme);
        }

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
