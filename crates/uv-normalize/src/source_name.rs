#[cfg(feature = "serde")]
use serde::{Deserialize, Deserializer, Serialize};
use std::fmt;
use std::fmt::{Display, Formatter};
use std::str::FromStr;

use crate::InvalidNameError;

/// The normalized name of the source file for a dependency
#[cfg_attr(feature = "serde", derive(Serialize))]
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SourceName(String);

impl SourceName {
    /// Create a validated, normalized extra name.
    pub fn new(name: String) -> Self {
        Self(name)
    }
}

impl FromStr for SourceName {
    type Err = InvalidNameError;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        Ok(Self(name.to_string()))
    }
}

#[cfg(feature = "serde")]
impl<'de> Deserialize<'de> for SourceName {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::from_str(&s).map_err(serde::de::Error::custom)
    }
}

impl Display for SourceName {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl AsRef<str> for SourceName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}
