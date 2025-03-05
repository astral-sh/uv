use std::fmt::Formatter;
use std::str::FromStr;

use uv_pep440::{Version, VersionSpecifier, VersionSpecifiers, VersionSpecifiersParseError};

/// A required version of uv, represented as a version specifier (e.g. `>=0.5.0`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RequiredVersion(VersionSpecifiers);

impl RequiredVersion {
    /// Return `true` if the given version is required.
    pub fn contains(&self, version: &Version) -> bool {
        self.0.contains(version)
    }
}

impl FromStr for RequiredVersion {
    type Err = VersionSpecifiersParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Treat `0.5.0` as `==0.5.0`, for backwards compatibility.
        if let Ok(version) = Version::from_str(s) {
            Ok(Self(VersionSpecifiers::from(
                VersionSpecifier::equals_version(version),
            )))
        } else {
            Ok(Self(VersionSpecifiers::from_str(s)?))
        }
    }
}

#[cfg(feature = "schemars")]
impl schemars::JsonSchema for RequiredVersion {
    fn schema_name() -> String {
        String::from("RequiredVersion")
    }

    fn json_schema(_gen: &mut schemars::r#gen::SchemaGenerator) -> schemars::schema::Schema {
        schemars::schema::SchemaObject {
            instance_type: Some(schemars::schema::InstanceType::String.into()),
            metadata: Some(Box::new(schemars::schema::Metadata {
                description: Some("A version specifier, e.g. `>=0.5.0` or `==0.5.0`.".to_string()),
                ..schemars::schema::Metadata::default()
            })),
            ..schemars::schema::SchemaObject::default()
        }
        .into()
    }
}

impl<'de> serde::Deserialize<'de> for RequiredVersion {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;

        impl serde::de::Visitor<'_> for Visitor {
            type Value = RequiredVersion;

            fn expecting(&self, f: &mut Formatter) -> std::fmt::Result {
                f.write_str("a string")
            }

            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
                RequiredVersion::from_str(v).map_err(serde::de::Error::custom)
            }
        }

        deserializer.deserialize_str(Visitor)
    }
}

impl std::fmt::Display for RequiredVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.0, f)
    }
}
