use serde::{Deserialize, Serialize};
use uv_redacted::SafeUrl;
use uv_redacted::policy::Raw;

#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct BaseUrl(
    #[serde(
        serialize_with = "SafeUrl::<Raw>::serialize_internal",
        deserialize_with = "SafeUrl::<Raw>::deserialize_internal"
    )]
    SafeUrl<Raw>,
);

impl BaseUrl {
    /// Return the underlying [`SafeUrl`].
    pub fn as_url(&self) -> &SafeUrl {
        self.0.as_url()
    }

    /// Return the underlying [`SafeUrl`] as a serialized string.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<SafeUrl> for BaseUrl {
    fn from(url: SafeUrl) -> Self {
        Self(url.with_policy(Raw))
    }
}

impl std::fmt::Display for BaseUrl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[cfg(test)]
#[expect(
    clippy::disallowed_types,
    reason = "Compares internal serialization with the `url` crate wire format"
)]
mod tests {
    use std::error::Error;

    use serde::Serialize;
    use url::Url;
    use uv_redacted::SafeUrl;

    use super::BaseUrl;

    #[test]
    fn internal_serialization_format() -> Result<(), Box<dyn Error>> {
        #[derive(Serialize)]
        struct InternalUrl(#[serde(serialize_with = "Url::serialize_internal")] Url);

        let url = Url::parse("https://user:password@example.com/simple?sig=abc%2Bdef%3D")?;
        let expected = serde_json::to_value(InternalUrl(url.clone()))?;
        let base = BaseUrl::from(SafeUrl::from_url(url));
        let serialized = serde_json::to_value(&base)?;
        assert_eq!(serialized, expected);

        let roundtrip: BaseUrl = serde_json::from_value(serialized)?;
        assert_eq!(roundtrip, base);
        Ok(())
    }
}
