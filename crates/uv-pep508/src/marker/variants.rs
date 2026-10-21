use std::fmt::{self, Display};
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// An invalid component of a wheel variant property.
#[derive(Debug, thiserror::Error)]
pub enum VariantParseError {
    /// The namespace is invalid.
    #[error("Invalid variant namespace: `{0}`")]
    InvalidNamespace(String),
    /// The feature is invalid.
    #[error("Invalid variant feature: `{0}`")]
    InvalidFeature(String),
    /// The value is invalid.
    #[error("Invalid variant value: `{0}`")]
    InvalidValue(String),
}

/// A validated variant namespace.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String")]
pub struct VariantNamespace(String);

impl VariantNamespace {
    /// Return the namespace as a string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for VariantNamespace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for VariantNamespace {
    type Error = VariantParseError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if valid_component(&value, false) {
            Ok(Self(value))
        } else {
            Err(VariantParseError::InvalidNamespace(value))
        }
    }
}

impl FromStr for VariantNamespace {
    type Err = VariantParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::try_from(value.trim().to_owned())
    }
}

/// A validated variant feature.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String")]
pub struct VariantFeature(String);

impl VariantFeature {
    /// Return the feature as a string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for VariantFeature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for VariantFeature {
    type Error = VariantParseError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if valid_component(&value, false) {
            Ok(Self(value))
        } else {
            Err(VariantParseError::InvalidFeature(value))
        }
    }
}

impl FromStr for VariantFeature {
    type Err = VariantParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::try_from(value.trim().to_owned())
    }
}

/// A validated variant value.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String")]
pub struct VariantValue(String);

impl VariantValue {
    /// Return the value as a string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for VariantValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for VariantValue {
    type Error = VariantParseError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if valid_component(&value, true) {
            Ok(Self(value))
        } else {
            Err(VariantParseError::InvalidValue(value))
        }
    }
}

impl FromStr for VariantValue {
    type Err = VariantParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::try_from(value.trim().to_owned())
    }
}

fn valid_component(value: &str, allow_period: bool) -> bool {
    !value.is_empty()
        && value.bytes().all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || character == b'_'
                || (allow_period && character == b'.')
        })
}
