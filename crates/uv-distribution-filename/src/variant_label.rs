use serde::{Deserialize, Deserializer};
use std::fmt::Display;
use std::str::FromStr;
use uv_small_str::SmallString;

#[derive(Debug, thiserror::Error)]
pub enum InvalidVariantLabel {
    #[error("Invalid character `{invalid}` in variant label, only [a-z0-9._] are allowed: {input}")]
    InvalidCharacter { invalid: char, input: String },
    #[error("Variant label must be between 1 and 8 characters long, not {length}: {input}")]
    InvalidLength { length: usize, input: String },
}

#[derive(
    Debug,
    Clone,
    Eq,
    PartialEq,
    Hash,
    Ord,
    PartialOrd,
    serde::Serialize,
    rkyv::Archive,
    rkyv::Deserialize,
    rkyv::Serialize,
)]
#[rkyv(derive(Debug))]
#[serde(transparent)]
pub struct VariantLabel(SmallString);

impl FromStr for VariantLabel {
    type Err = InvalidVariantLabel;

    fn from_str(label: &str) -> Result<Self, Self::Err> {
        if let Some(invalid) = label
            .chars()
            .find(|c| !(c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '.'))
        {
            if !invalid.is_ascii_lowercase() && !invalid.is_ascii_digit() && invalid != '.' {
                return Err(InvalidVariantLabel::InvalidCharacter {
                    invalid,
                    input: label.to_string(),
                });
            }
        }

        // We checked that the label is ASCII only above, so we can use `len()`.
        if label.is_empty() || label.len() > 8 {
            return Err(InvalidVariantLabel::InvalidLength {
                length: label.len(),
                input: label.to_string(),
            });
        }

        Ok(Self(SmallString::from(label)))
    }
}

impl<'de> Deserialize<'de> for VariantLabel {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::from_str(&s).map_err(serde::de::Error::custom)
    }
}

impl Display for VariantLabel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.0, f)
    }
}
