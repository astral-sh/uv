use std::str::FromStr;
use uv_auth::{self, KeyringProvider};
use uv_preview::{Preview, PreviewFeatures};
use uv_redacted::DisplaySafeUrl;
use uv_warnings::warn_user_once;

/// Keyring provider type to use for credential lookup.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum KeyringProviderType {
    /// Do not use keyring for credential lookup.
    #[default]
    Disabled,
    /// Use a native integration with the system keychain for credential lookup.
    Native,
    /// Use the `keyring` command for credential lookup.
    Subprocess,
    // /// Not yet implemented
    // Auto,
    // /// Not implemented yet. Maybe use <https://docs.rs/keyring/latest/keyring/> for this?
    // Import,
}
// See <https://pip.pypa.io/en/stable/topics/authentication/#keyring-support> for details.

impl KeyringProviderType {
    pub fn to_provider(&self, preview: &Preview) -> Option<KeyringProvider> {
        match self {
            Self::Disabled => None,
            Self::Native => {
                if !preview.is_enabled(PreviewFeatures::NATIVE_KEYRING) {
                    warn_user_once!(
                        "The native keyring provider is experimental and may change without warning. Pass `--preview-features {}` to disable this warning.",
                        PreviewFeatures::NATIVE_KEYRING
                    );
                }
                Some(KeyringProvider::native())
            }
            Self::Subprocess => Some(KeyringProvider::subprocess()),
        }
    }
}

impl std::fmt::Display for KeyringProviderType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Disabled => write!(f, "disabled"),
            Self::Native => write!(f, "native"),
            Self::Subprocess => write!(f, "subprocess"),
        }
    }
}

#[derive(thiserror::Error, Debug)]
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
#[derive(Debug, Clone)]
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

        // Only allow HTTPS URLs
        if url.scheme() != "https" {
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
