//! Explicit serialization policies for persisted URLs.
//!
//! Deserialization accepts any syntactically valid URL, without applying human-input heuristics to
//! paths in stored data or protocol messages. Human input should first be parsed as [`DisplaySafeUrl`].

use std::fmt::{Debug, Display, Formatter};

use ref_cast::RefCast;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::DisplaySafeUrl;

/// A URL whose serialized representation retains all authentication and query parameters.
///
/// Use for replayable artifact locations and internal data. Display and debug output remain redacted.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize, RefCast)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "schemars", schemars(transparent))]
#[serde(transparent)]
#[repr(transparent)]
pub struct UrlWithCredentials(
    #[serde(deserialize_with = "DisplaySafeUrl::deserialize_from_url")] DisplaySafeUrl,
);

impl UrlWithCredentials {
    /// Borrow a URL with this persistence policy.
    pub fn ref_cast(url: &DisplaySafeUrl) -> &Self {
        RefCast::ref_cast(url)
    }

    /// Return the original URL for requests and source comparisons.
    pub fn as_url(&self) -> &DisplaySafeUrl {
        &self.0
    }

    /// Return the original URL for requests and source comparisons.
    pub fn into_url(self) -> DisplaySafeUrl {
        self.0
    }
}

impl From<DisplaySafeUrl> for UrlWithCredentials {
    fn from(url: DisplaySafeUrl) -> Self {
        Self(url)
    }
}

impl Serialize for UrlWithCredentials {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        Url::serialize(&self.0, serializer)
    }
}

impl Debug for UrlWithCredentials {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(&self.0, formatter)
    }
}

impl Display for UrlWithCredentials {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.0, formatter)
    }
}

/// A URL whose serialized representation omits sensitive userinfo and retains query parameters.
///
/// Use for installed direct URL metadata, where query parameters identify the source. The generic
/// SSH `git` username is retained. Display and debug output remain redacted.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "schemars", schemars(transparent))]
#[serde(transparent)]
pub struct UrlWithoutUserInfo(
    #[serde(deserialize_with = "DisplaySafeUrl::deserialize_from_url")] DisplaySafeUrl,
);

impl From<DisplaySafeUrl> for UrlWithoutUserInfo {
    fn from(url: DisplaySafeUrl) -> Self {
        Self(url)
    }
}

impl Serialize for UrlWithoutUserInfo {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.0.without_userinfo().serialize(serializer)
    }
}

impl Debug for UrlWithoutUserInfo {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(&self.0, formatter)
    }
}

impl Display for UrlWithoutUserInfo {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.0, formatter)
    }
}

/// A URL whose serialized representation omits sensitive userinfo and recognized query credentials.
///
/// Use for persisted index and configuration references. Deserialization retains authentication
/// for requests; serialization can produce a URL that requires credentials to be supplied separately.
/// Display and debug output remain redacted.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize, RefCast)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "schemars", schemars(transparent))]
#[serde(transparent)]
#[repr(transparent)]
pub struct UrlWithoutSensitiveParts(
    #[serde(deserialize_with = "DisplaySafeUrl::deserialize_from_url")] DisplaySafeUrl,
);

impl UrlWithoutSensitiveParts {
    /// Borrow a URL with this persistence policy.
    pub fn ref_cast(url: &DisplaySafeUrl) -> &Self {
        RefCast::ref_cast(url)
    }

    /// Return the original URL for requests and source comparisons.
    pub fn as_url(&self) -> &DisplaySafeUrl {
        &self.0
    }

    /// Return the original URL for requests and source comparisons.
    pub fn into_url(self) -> DisplaySafeUrl {
        self.0
    }
}

impl From<DisplaySafeUrl> for UrlWithoutSensitiveParts {
    fn from(url: DisplaySafeUrl) -> Self {
        Self(url)
    }
}

impl Serialize for UrlWithoutSensitiveParts {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.0.without_sensitive_parts().serialize(serializer)
    }
}

impl Debug for UrlWithoutSensitiveParts {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(&self.0, formatter)
    }
}

impl Display for UrlWithoutSensitiveParts {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.0, formatter)
    }
}

impl AsRef<str> for UrlWithoutUserInfo {
    fn as_ref(&self) -> &str {
        self.0.as_str()
    }
}

#[cfg(test)]
mod tests {
    use serde::Serialize;

    use super::{UrlWithCredentials, UrlWithoutSensitiveParts, UrlWithoutUserInfo};
    use crate::DisplaySafeUrl;

    #[test]
    fn deserialize_persisted_url() -> Result<(), serde_json::Error> {
        let input = "https://example.com/package:version@revision?sig=abc%2Bdef%3D";
        let serialized = serde_json::to_string(input)?;
        assert_eq!(
            serde_json::from_str::<UrlWithCredentials>(&serialized)?
                .as_url()
                .as_str(),
            input
        );
        assert_eq!(
            serde_json::from_str::<UrlWithoutUserInfo>(&serialized)?.as_ref(),
            input
        );
        assert_eq!(
            serde_json::from_str::<UrlWithoutSensitiveParts>(&serialized)?
                .as_url()
                .as_str(),
            input
        );
        assert!(serde_json::from_str::<DisplaySafeUrl>(&serialized).is_err());
        Ok(())
    }

    #[test]
    fn persistence_policies() -> Result<(), Box<dyn std::error::Error>> {
        #[derive(Serialize)]
        struct Policies<'a> {
            with_credentials: &'a UrlWithCredentials,
            without_userinfo: &'a UrlWithoutUserInfo,
            without_sensitive_parts: &'a UrlWithoutSensitiveParts,
        }

        let url = DisplaySafeUrl::parse(
            "https://user:password@example.com/package.whl?st=2026-09-15T16:34:14Z&Si%67=abc%2Bdef%3D&keep=%2f&keep=a+b&X-Amz-Security-Token=token",
        )?;
        let without_userinfo = UrlWithoutUserInfo::from(url.clone());
        let policies = Policies {
            with_credentials: UrlWithCredentials::ref_cast(&url),
            without_userinfo: &without_userinfo,
            without_sensitive_parts: UrlWithoutSensitiveParts::ref_cast(&url),
        };
        insta::assert_snapshot!(serde_json::to_string_pretty(&policies)?, @r#"
        {
          "with_credentials": "https://user:password@example.com/package.whl?st=2026-09-15T16:34:14Z&Si%67=abc%2Bdef%3D&keep=%2f&keep=a+b&X-Amz-Security-Token=token",
          "without_userinfo": "https://example.com/package.whl?st=2026-09-15T16:34:14Z&Si%67=abc%2Bdef%3D&keep=%2f&keep=a+b&X-Amz-Security-Token=token",
          "without_sensitive_parts": "https://example.com/package.whl?st=2026-09-15T16:34:14Z&keep=%2f&keep=a+b"
        }
        "#);

        // Persistence policies must not change the credentials available to requests.
        let serialized = serde_json::to_string(policies.with_credentials)?;
        assert_eq!(
            serde_json::from_str::<UrlWithCredentials>(&serialized)?.as_url(),
            &url
        );
        assert_eq!(
            serde_json::from_str::<UrlWithoutUserInfo>(&serialized)?.as_ref(),
            url.as_str()
        );
        assert_eq!(
            serde_json::from_str::<UrlWithoutSensitiveParts>(&serialized)?.as_url(),
            &url
        );

        // Every persistence policy remains safe to display.
        assert_eq!(policies.with_credentials.to_string(), url.to_string());
        assert_eq!(policies.without_userinfo.to_string(), url.to_string());
        assert_eq!(
            policies.without_sensitive_parts.to_string(),
            url.to_string()
        );
        assert_eq!(
            format!("{:?}", policies.with_credentials),
            format!("{url:?}")
        );
        assert_eq!(
            format!("{:?}", policies.without_userinfo),
            format!("{url:?}")
        );
        assert_eq!(
            format!("{:?}", policies.without_sensitive_parts),
            format!("{url:?}")
        );
        Ok(())
    }
}
