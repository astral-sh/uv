//! Type-level policies for ordinary URL serialization.
//!
//! Policies govern Serde serialization of the normalized URL, not its original
//! input bytes. Changing a policy leaves the parsed URL and its credentials in
//! memory. Display and debug output use the same redaction for every policy;
//! ordinary deserialization uses URL parsing independently of the policy.

use std::hash::Hash;

use serde::Serialize;

use crate::SafeUrl;

/// Controls whether and how [`SafeUrl`] implements ordinary Serde serialization.
///
/// This trait is sealed so callers choose from the policies defined here.
pub trait SerializationPolicy: private::Sealed + Copy + Ord + Hash {}

/// A policy that permits serializing the complete internal URL representation.
///
/// Only [`Raw`] implements this trait.
pub trait InternalSerializationPolicy: SerializationPolicy {}

/// Do not implement ordinary Serde [`Serialize`].
///
/// This is the default policy for [`SafeUrl`]. A serializing policy must be
/// selected to serialize a [`SafeUrl`] using its [`Serialize`] implementation.
/// The URL remains accessible and can still be written by other means.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Unserializable;

/// Remove only sensitive userinfo when serializing the normalized URL.
///
/// A passwordless `git` username is retained for `ssh`, `git+ssh`, and `git+https`
/// URLs. The complete query is retained, including any signed credentials.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Stripped;

/// Serialize the complete normalized URL, including userinfo and query credentials.
///
/// This uses the parsed URL, not the original input bytes. Display and debug
/// output remain redacted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Raw;

impl SerializationPolicy for Unserializable {}
impl SerializationPolicy for Stripped {}
impl SerializationPolicy for Raw {}

impl InternalSerializationPolicy for Raw {}

mod private {
    pub trait Sealed {}

    impl Sealed for super::Unserializable {}
    impl Sealed for super::Stripped {}
    impl Sealed for super::Raw {}
}

impl Serialize for SafeUrl<Stripped> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.without_credentials().as_str())
    }
}

impl Serialize for SafeUrl<Raw> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;
    use std::error::Error;

    use insta::assert_snapshot;
    use serde::{Deserialize, Serialize};

    use super::{Raw, Stripped, Unserializable};
    use crate::SafeUrl;

    #[test]
    fn policy_transformations_preserve_url_and_redaction() -> Result<(), Box<dyn Error>> {
        let input = "https://user:password@example.com/file.whl?st=2026-09-15T16:34:14Z&sig=abc%2Bdef%3D&other=a+b";
        let url = SafeUrl::parse(input)?;
        let displayed = url.to_string();
        let debugged = format!("{url:?}");
        let stripped = url.with_policy(Stripped);
        assert_snapshot!(serde_json::to_string(&stripped)?, @r#""https://example.com/file.whl?st=2026-09-15T16:34:14Z&sig=abc%2Bdef%3D&other=a+b""#);
        assert_eq!(stripped.as_str(), input);
        assert_eq!(stripped.to_string(), displayed);
        assert_eq!(format!("{stripped:?}"), debugged);

        let preserved = stripped.with_policy(Raw);
        assert_eq!(
            serde_json::to_string(&preserved)?,
            serde_json::to_string(input)?
        );
        assert_eq!(preserved.to_string(), displayed);
        assert_eq!(format!("{preserved:?}"), debugged);
        assert_eq!(preserved.with_policy(Unserializable).as_str(), input);
        Ok(())
    }

    #[test]
    fn credential_removal_retains_policy_and_query() -> Result<(), Box<dyn Error>> {
        let input = "https://user:password@example.com/file.whl?sig=abc%2Bdef%3D&other=a+b";
        let expected = "https://example.com/file.whl?sig=abc%2Bdef%3D&other=a+b";
        let url = SafeUrl::parse(input)?;
        let removed: Cow<'_, SafeUrl> = url.without_credentials();
        assert_eq!(removed.as_str(), expected);
        assert_snapshot!(removed.to_string(), @"https://example.com/file.whl?sig=****&other=a+b");
        assert_snapshot!(format!("{removed:?}"), @r#"SafeUrl { scheme: "https", cannot_be_a_base: false, username: "", password: None, host: Some(Domain("example.com")), port: None, path: "/file.whl", query: Some("sig=****&other=a+b"), fragment: None }"#);
        assert_eq!(url.as_str(), input);

        let stripped = url.clone().with_policy(Stripped);
        let removed: Cow<'_, SafeUrl<Stripped>> = stripped.without_credentials();
        assert_eq!(
            serde_json::to_string(&removed)?,
            serde_json::to_string(expected)?
        );

        let raw = url.with_policy(Raw);
        let removed: Cow<'_, SafeUrl<Raw>> = raw.without_credentials();
        assert_eq!(
            serde_json::to_string(&removed)?,
            serde_json::to_string(expected)?
        );
        Ok(())
    }

    #[test]
    fn mutation_guards_retain_policy_and_url_components() -> Result<(), Box<dyn Error>> {
        let mut url = SafeUrl::parse(
            "https://user:password@example.com/packages/?download=1#subdirectory=src",
        )?
        .with_policy(Stripped);
        url.path_segments_mut()
            .map_err(|()| "URL has no path segments")?
            .pop_if_empty()
            .push("file name.whl");
        url.query_pairs_mut().append_pair("sig", "signed value");

        assert_eq!(
            url.as_str(),
            "https://user:password@example.com/packages/file%20name.whl?download=1&sig=signed+value#subdirectory=src"
        );
        assert_snapshot!(url, @"https://user:****@example.com/packages/file%20name.whl?download=1&sig=****#subdirectory=src");
        assert_snapshot!(serde_json::to_string(&url)?, @r#""https://example.com/packages/file%20name.whl?download=1&sig=signed+value#subdirectory=src""#);
        Ok(())
    }

    #[test]
    fn method_serialization_respects_policy() -> Result<(), Box<dyn Error>> {
        let input = "https://user:password@example.com/file.whl?sig=abc%2Bdef%3D";
        let url = SafeUrl::parse(input)?;
        let stripped = url.clone().with_policy(Stripped);
        assert_snapshot!(stripped.serialize(serde_json::value::Serializer)?, @r#""https://example.com/file.whl?sig=abc%2Bdef%3D""#);

        let raw = url.with_policy(Raw);
        assert_eq!(
            raw.serialize(serde_json::value::Serializer)?,
            serde_json::Value::String(input.to_owned())
        );
        assert_eq!(
            raw.serialize_internal(serde_json::value::Serializer)?,
            raw.0.serialize_internal(serde_json::value::Serializer)?
        );
        Ok(())
    }

    #[test]
    fn borrowed_policy_retains_git_username() -> Result<(), Box<dyn Error>> {
        let url = SafeUrl::parse("ssh://git@github.com/astral-sh/uv?sig=abc%2Bdef%3D")?;
        assert_snapshot!(serde_json::to_string(url.with_policy_ref(Stripped))?, @r#""ssh://git@github.com/astral-sh/uv?sig=abc%2Bdef%3D""#);
        assert_snapshot!(url.without_credentials().to_string(), @"ssh://git@github.com/astral-sh/uv?sig=****");
        Ok(())
    }

    #[test]
    fn field_policy_survives_serialization_round_trip() -> Result<(), Box<dyn Error>> {
        #[derive(Serialize, Deserialize)]
        struct Origin {
            url: SafeUrl<Stripped>,
        }

        let origin = Origin {
            url: SafeUrl::parse(
                "https://user:password@example.com/files/project:build@nightly?sig=abc%2Bdef%3D",
            )?
            .with_policy(Stripped),
        };
        let serialized = serde_json::to_string(&origin)?;
        let deserialized: Origin = serde_json::from_str(&serialized)?;
        assert_snapshot!(serialized, @r#"{"url":"https://example.com/files/project:build@nightly?sig=abc%2Bdef%3D"}"#);
        assert_eq!(serde_json::to_string(&deserialized)?, serialized);
        assert!(SafeUrl::parse(deserialized.url.as_str()).is_err());
        Ok(())
    }
}
