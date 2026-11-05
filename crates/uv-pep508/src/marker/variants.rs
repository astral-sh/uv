use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fmt::{self, Display};
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// Variant properties, grouped by namespace and feature.
pub type VariantProperties =
    BTreeMap<VariantNamespace, BTreeMap<VariantFeature, Vec<VariantValue>>>;

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
pub struct VariantNamespace(Cow<'static, str>);

impl VariantNamespace {
    /// Return the namespace as a string.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Create a variant namespace from a static string.
    ///
    /// An invalid namespace causes a compilation error in a const context and a panic at runtime.
    pub const fn from_static(value: &'static str) -> Self {
        assert!(valid_component(value, false), "invalid variant namespace");
        Self(Cow::Borrowed(value))
    }
}

impl Display for VariantNamespace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Display::fmt(&self.0, f)
    }
}

impl FromStr for VariantNamespace {
    type Err = VariantParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::try_from(value.to_owned())
    }
}

impl TryFrom<String> for VariantNamespace {
    type Error = VariantParseError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if valid_component(&value, false) {
            Ok(Self(Cow::Owned(value)))
        } else {
            Err(VariantParseError::InvalidNamespace(value))
        }
    }
}

/// A validated variant feature.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String")]
pub struct VariantFeature(Cow<'static, str>);

impl VariantFeature {
    /// Return the feature as a string.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Create a variant feature from a static string.
    ///
    /// An invalid feature causes a compilation error in a const context and a panic at runtime.
    pub const fn from_static(value: &'static str) -> Self {
        assert!(valid_component(value, false), "invalid variant feature");
        Self(Cow::Borrowed(value))
    }
}

impl Display for VariantFeature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Display::fmt(&self.0, f)
    }
}

impl FromStr for VariantFeature {
    type Err = VariantParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::try_from(value.to_owned())
    }
}

impl TryFrom<String> for VariantFeature {
    type Error = VariantParseError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if valid_component(&value, false) {
            Ok(Self(Cow::Owned(value)))
        } else {
            Err(VariantParseError::InvalidFeature(value))
        }
    }
}

/// A validated variant value.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String")]
pub struct VariantValue(Cow<'static, str>);

impl VariantValue {
    /// Return the value as a string.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Create a variant value from a static string.
    ///
    /// An invalid value causes a compilation error in a const context and a panic at runtime.
    pub const fn from_static(value: &'static str) -> Self {
        assert!(valid_component(value, true), "invalid variant value");
        Self(Cow::Borrowed(value))
    }
}

impl Display for VariantValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Display::fmt(&self.0, f)
    }
}

impl FromStr for VariantValue {
    type Err = VariantParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::try_from(value.to_owned())
    }
}

impl TryFrom<String> for VariantValue {
    type Error = VariantParseError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if valid_component(&value, true) {
            Ok(Self(Cow::Owned(value)))
        } else {
            Err(VariantParseError::InvalidValue(value))
        }
    }
}

const fn valid_component(value: &str, allow_period: bool) -> bool {
    if value.is_empty() {
        return false;
    }
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        let character = bytes[index];
        if !(character.is_ascii_lowercase()
            || character.is_ascii_digit()
            || character == b'_'
            || (allow_period && character == b'.'))
        {
            return false;
        }
        index += 1;
    }
    true
}
