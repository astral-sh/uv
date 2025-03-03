use std::fmt;
use std::fmt::{Display, Formatter};
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize};

use uv_small_str::SmallString;

use crate::{validate_and_normalize_ref, InvalidNameError};

/// The normalized name of an extra dependency.
///
/// Converts the name to lowercase and collapses runs of `-`, `_`, and `.` down to a single `-`.
/// For example, `---`, `.`, and `__` are all converted to a single `-`.
///
/// See:
/// - <https://peps.python.org/pep-0685/#specification/>
/// - <https://packaging.python.org/en/latest/specifications/name-normalization/>
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct ExtraName(SmallString);

impl ExtraName {
    /// Create a validated, normalized extra name.
    ///
    /// At present, this is no more efficient than calling [`ExtraName::from_str`].
    #[allow(clippy::needless_pass_by_value)]
    pub fn from_owned(name: String) -> Result<Self, InvalidNameError> {
        validate_and_normalize_ref(&name).map(Self)
    }

    /// Return the underlying extra name as a string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for ExtraName {
    type Err = InvalidNameError;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        validate_and_normalize_ref(name).map(Self)
    }
}

impl<'de> Deserialize<'de> for ExtraName {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct Visitor;

        impl serde::de::Visitor<'_> for Visitor {
            type Value = ExtraName;

            fn expecting(&self, f: &mut Formatter) -> fmt::Result {
                f.write_str("a string")
            }

            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
                ExtraName::from_str(v).map_err(serde::de::Error::custom)
            }

            fn visit_string<E: serde::de::Error>(self, v: String) -> Result<Self::Value, E> {
                ExtraName::from_owned(v).map_err(serde::de::Error::custom)
            }
        }

        deserializer.deserialize_str(Visitor)
    }
}

impl Display for ExtraName {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl AsRef<str> for ExtraName {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}
