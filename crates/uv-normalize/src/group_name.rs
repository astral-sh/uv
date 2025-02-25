use std::str::FromStr;
use std::sync::LazyLock;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use uv_small_str::SmallString;

use crate::{validate_and_normalize_ref, InvalidNameError};

/// The normalized name of a dependency group.
///
/// See:
/// - <https://peps.python.org/pep-0735/>
/// - <https://packaging.python.org/en/latest/specifications/name-normalization/>
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct GroupName(SmallString);

impl GroupName {
    /// Create a validated, normalized group name.
    ///
    /// At present, this is no more efficient than calling [`GroupName::from_str`].
    #[allow(clippy::needless_pass_by_value)]
    pub fn from_owned(name: String) -> Result<Self, InvalidNameError> {
        validate_and_normalize_ref(&name).map(Self)
    }
}

impl FromStr for GroupName {
    type Err = InvalidNameError;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        validate_and_normalize_ref(name).map(Self)
    }
}

impl<'de> Deserialize<'de> for GroupName {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct Visitor;

        impl serde::de::Visitor<'_> for Visitor {
            type Value = GroupName;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a string")
            }

            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
                GroupName::from_str(v).map_err(serde::de::Error::custom)
            }

            fn visit_string<E: serde::de::Error>(self, v: String) -> Result<Self::Value, E> {
                GroupName::from_owned(v).map_err(serde::de::Error::custom)
            }
        }

        deserializer.deserialize_str(Visitor)
    }
}

impl Serialize for GroupName {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.0.serialize(serializer)
    }
}

impl std::fmt::Display for GroupName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl AsRef<str> for GroupName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// The name of the global `dev-dependencies` group.
///
/// Internally, we model dependency groups as a generic concept; but externally, we only expose the
/// `dev-dependencies` group.
pub static DEV_DEPENDENCIES: LazyLock<GroupName> =
    LazyLock::new(|| GroupName::from_str("dev").unwrap());
