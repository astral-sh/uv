use std::fmt::Display;
use std::str::FromStr;

use serde::{Deserialize, Deserializer};
use thiserror::Error;

// The parser and errors are also used for parsing JSON files, so we're keeping the error message
// generic without references to markers.
/// A segment of a variant uses invalid characters.
#[derive(Error, Debug)]
pub enum VariantParseError {
    /// The namespace segment of a variant failed to parse.
    #[error(
        "Invalid character `{invalid}` in variant namespace, only [a-z0-9_] are allowed: {input}"
    )]
    Namespace {
        /// The character outside the allowed character range.
        invalid: char,
        /// The invalid input string.
        input: String,
    },
    #[error(
        "Invalid character `{invalid}` in variant feature, only [a-z0-9_] are allowed: {input}"
    )]
    /// The feature segment of a variant failed to parse.
    Feature {
        /// The character outside the allowed character range.
        invalid: char,
        /// The invalid input string.
        input: String,
    },
    #[error(
        "Invalid character `{invalid}` in variant value, only [a-z0-9_.,!>~<=] are allowed: {input}"
    )]
    /// The value segment of a variant failed to parse.
    Value {
        /// The character outside the allowed character range.
        invalid: char,
        /// The invalid input string.
        input: String,
    },
}

/// The namespace segment in a variant.
///
/// Variant properties have the structure `namespace :: feature ::value`.
///
/// The segment is canonicalized by trimming it.
#[derive(Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct VariantNamespace(String);

impl FromStr for VariantNamespace {
    type Err = VariantParseError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let input = input.trim();
        if let Some(invalid) = input
            .chars()
            .find(|c| !(c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '_'))
        {
            return Err(VariantParseError::Namespace {
                invalid,
                input: input.to_string(),
            });
        }

        Ok(Self(input.to_string()))
    }
}

impl<'de> Deserialize<'de> for VariantNamespace {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::from_str(&s).map_err(serde::de::Error::custom)
    }
}

impl Display for VariantNamespace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.0, f)
    }
}

/// The feature segment in a variant.
///
/// Variant properties have the structure `namespace :: feature ::value`.
///
/// The segment is canonicalized by trimming it.
#[derive(Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct VariantFeature(String);

impl FromStr for VariantFeature {
    type Err = VariantParseError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let input = input.trim();
        if let Some(invalid) = input
            .chars()
            .find(|c| !(c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '_'))
        {
            return Err(VariantParseError::Feature {
                invalid,
                input: input.to_string(),
            });
        }

        Ok(Self(input.to_string()))
    }
}

impl<'de> Deserialize<'de> for VariantFeature {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::from_str(&s).map_err(serde::de::Error::custom)
    }
}

impl Display for VariantFeature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.0, f)
    }
}

/// The value segment in a variant.
///
/// Variant properties have the structure `namespace :: feature ::value`.
///
/// The segment is canonicalized by trimming it.
#[derive(Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct VariantValue(String);

impl FromStr for VariantValue {
    type Err = VariantParseError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let input = input.trim();
        if let Some(invalid) = input.chars().find(|c| {
            !(c.is_ascii_lowercase()
                || c.is_ascii_digit()
                || matches!(*c, '_' | '.' | ',' | '!' | '>' | '~' | '<' | '='))
        }) {
            return Err(VariantParseError::Value {
                invalid,
                input: input.to_string(),
            });
        }

        Ok(Self(input.to_string()))
    }
}

impl<'de> Deserialize<'de> for VariantValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::from_str(&s).map_err(serde::de::Error::custom)
    }
}

impl Display for VariantValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.0, f)
    }
}
