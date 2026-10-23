use std::borrow::Cow;
use std::fmt::{Display, Formatter};
use std::str::FromStr;

use serde::{Deserialize, Deserializer};

/// An invalid wheel variant label.
#[derive(Debug, thiserror::Error)]
pub enum InvalidVariantLabel {
    #[error("Variant label must not be empty")]
    Empty,
    #[error(
        "Variant label must contain only lowercase ASCII letters, digits, underscores, and periods, not `{0}`"
    )]
    InvalidCharacters(char),
}

/// A wheel variant label.
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
pub struct VariantLabel(#[rkyv(with = rkyv::with::AsOwned)] Cow<'static, str>);

impl VariantLabel {
    /// Create a variant label from a static string.
    ///
    /// An invalid label causes a compilation error in a const context and a panic at runtime.
    pub const fn from_static(label: &'static str) -> Self {
        assert!(!label.is_empty(), "variant label must not be empty");
        let bytes = label.as_bytes();
        let mut index = 0;
        while index < bytes.len() {
            assert!(
                valid_character(bytes[index] as char),
                "invalid variant label"
            );
            index += 1;
        }
        Self(Cow::Borrowed(label))
    }
}

impl Display for VariantLabel {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.0, f)
    }
}

impl FromStr for VariantLabel {
    type Err = InvalidVariantLabel;

    fn from_str(label: &str) -> Result<Self, Self::Err> {
        Self::try_from(label.to_owned())
    }
}

impl TryFrom<String> for VariantLabel {
    type Error = InvalidVariantLabel;

    fn try_from(label: String) -> Result<Self, Self::Error> {
        if label.is_empty() {
            return Err(InvalidVariantLabel::Empty);
        }
        if let Some(character) = label.chars().find(|&character| !valid_character(character)) {
            return Err(InvalidVariantLabel::InvalidCharacters(character));
        }
        Ok(Self(Cow::Owned(label)))
    }
}

impl<'de> Deserialize<'de> for VariantLabel {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::try_from(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

const fn valid_character(character: char) -> bool {
    character.is_ascii_lowercase() || character.is_ascii_digit() || matches!(character, '.' | '_')
}
