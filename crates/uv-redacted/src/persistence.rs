use std::fmt::{Debug, Display, Formatter};

use ref_cast::RefCast;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::DisplaySafeUrl;

/// A URL that retains credentials when serialized.
///
/// The full parsed URL is retained in memory. Display and debug output use
/// [`DisplaySafeUrl`]'s redaction. Serialization preserves the entire normalized
/// URL, including its username, password, and query. Deserialization uses [`Url`]
/// parsing without the ambiguity checks applied to human input.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, RefCast)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "schemars", schemars(transparent))]
#[repr(transparent)]
pub struct CredentialPersistingUrl(DisplaySafeUrl);

impl CredentialPersistingUrl {
    /// Borrow a [`DisplaySafeUrl`] as a [`CredentialPersistingUrl`].
    pub fn ref_cast(url: &DisplaySafeUrl) -> &Self {
        RefCast::ref_cast(url)
    }

    /// Borrow the underlying [`DisplaySafeUrl`].
    pub fn as_url(&self) -> &DisplaySafeUrl {
        &self.0
    }

    /// Return the underlying [`DisplaySafeUrl`].
    pub fn into_url(self) -> DisplaySafeUrl {
        self.0
    }
}

impl From<DisplaySafeUrl> for CredentialPersistingUrl {
    fn from(url: DisplaySafeUrl) -> Self {
        Self(url)
    }
}

impl Serialize for CredentialPersistingUrl {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.0.as_str())
    }
}

impl<'de> Deserialize<'de> for CredentialPersistingUrl {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Url::deserialize(deserializer)
            .map(DisplaySafeUrl::from_url)
            .map(Self)
    }
}

impl Debug for CredentialPersistingUrl {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(&self.0, formatter)
    }
}

impl Display for CredentialPersistingUrl {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.0, formatter)
    }
}

/// A URL that removes sensitive userinfo when serialized.
///
/// The full parsed URL is retained in memory. Display and debug output use
/// [`DisplaySafeUrl`]'s redaction. Serialization preserves the entire query and
/// retains a passwordless `git` username for `ssh`, `git+ssh`, and `git+https` URLs.
/// Deserialization uses [`Url`] parsing without the ambiguity checks applied to
/// human input.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, RefCast)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "schemars", schemars(transparent))]
#[repr(transparent)]
pub struct PersistSafeUrl(DisplaySafeUrl);

impl PersistSafeUrl {
    /// Borrow a [`DisplaySafeUrl`] as a [`PersistSafeUrl`].
    pub fn ref_cast(url: &DisplaySafeUrl) -> &Self {
        RefCast::ref_cast(url)
    }

    /// Borrow the underlying [`DisplaySafeUrl`].
    pub fn as_url(&self) -> &DisplaySafeUrl {
        &self.0
    }

    /// Return the underlying [`DisplaySafeUrl`].
    pub fn into_url(self) -> DisplaySafeUrl {
        self.0
    }
}

impl From<DisplaySafeUrl> for PersistSafeUrl {
    fn from(url: DisplaySafeUrl) -> Self {
        Self(url)
    }
}

impl AsRef<str> for PersistSafeUrl {
    fn as_ref(&self) -> &str {
        self.0.as_str()
    }
}

impl Serialize for PersistSafeUrl {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.0.without_credentials().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for PersistSafeUrl {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Url::deserialize(deserializer)
            .map(DisplaySafeUrl::from_url)
            .map(Self)
    }
}

impl Debug for PersistSafeUrl {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(&self.0, formatter)
    }
}

impl Display for PersistSafeUrl {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.0, formatter)
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use insta::assert_snapshot;

    use super::{CredentialPersistingUrl, PersistSafeUrl};
    use crate::DisplaySafeUrl;

    #[test]
    fn credential_persistence_round_trip() -> Result<(), Box<dyn Error>> {
        let input = "https://user:password@example.com/files/project:build@nightly?sig=abc%2Bdef%3D&other=a+b";
        let url: CredentialPersistingUrl = serde_json::from_str(&serde_json::to_string(input)?)?;

        assert_snapshot!(serde_json::to_string(&url)?, @r#""https://user:password@example.com/files/project:build@nightly?sig=abc%2Bdef%3D&other=a+b""#);
        assert_eq!(url.as_url().as_str(), input);
        assert_eq!(url.to_string(), url.as_url().to_string());
        assert_eq!(format!("{url:?}"), format!("{:?}", url.as_url()));

        Ok(())
    }

    #[test]
    fn persistence_removes_userinfo_and_preserves_query() -> Result<(), Box<dyn Error>> {
        let input = "https://user:password@example.com/file.whl?st=2026-09-15T16:34:14Z&sig=abc%2Bdef%3D&other=a+b";
        let url = DisplaySafeUrl::parse(input)?;
        let persisted = PersistSafeUrl::ref_cast(&url);

        assert_snapshot!(serde_json::to_string(persisted)?, @r#""https://example.com/file.whl?st=2026-09-15T16:34:14Z&sig=abc%2Bdef%3D&other=a+b""#);
        assert_eq!(persisted.as_url().as_str(), input);
        assert_eq!(persisted.to_string(), url.to_string());
        assert_eq!(format!("{persisted:?}"), format!("{url:?}"));

        Ok(())
    }

    #[test]
    fn persistence_retains_git_username() -> Result<(), Box<dyn Error>> {
        let url = DisplaySafeUrl::parse("ssh://git@github.com/astral-sh/uv")?;
        assert_snapshot!(serde_json::to_string(PersistSafeUrl::ref_cast(&url))?, @r#""ssh://git@github.com/astral-sh/uv""#);
        Ok(())
    }

    #[test]
    fn persistence_deserializes_protocol_urls() -> Result<(), Box<dyn Error>> {
        let input = "https://example.com/files/project:build@nightly?sig=abc%2Bdef%3D";
        let serialized = serde_json::to_string(input)?;
        let url: CredentialPersistingUrl = serde_json::from_str(&serialized)?;
        assert_eq!(url.into_url().as_str(), input);
        assert!(serde_json::from_str::<DisplaySafeUrl>(&serialized).is_err());
        Ok(())
    }
}
