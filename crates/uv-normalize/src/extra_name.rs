use std::fmt;
use std::fmt::{Display, Formatter};
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize};

use crate::{validate_and_normalize_owned, validate_and_normalize_ref, InvalidNameError};

/// The normalized name of an extra dependency.
///
/// Converts the name to lowercase and collapses runs of `-`, `_`, and `.` down to a single `-`.
/// For example, `---`, `.`, and `__` are all converted to a single `-`.
///
/// See:
/// - <https://peps.python.org/pep-0685/#specification/>
/// - <https://packaging.python.org/en/latest/specifications/name-normalization/>
#[derive(Debug, Default, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct ExtraName(String);

impl ExtraName {
    /// Create a validated, normalized extra name.
    pub fn new(name: String) -> Result<Self, InvalidNameError> {
        validate_and_normalize_owned(name).map(Self)
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
        let s = String::deserialize(deserializer)?;
        Self::from_str(&s).map_err(serde::de::Error::custom)
    }
}

impl Display for ExtraName {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl AsRef<str> for ExtraName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}
