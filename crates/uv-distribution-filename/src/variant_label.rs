use std::fmt::{Display, Formatter};
use std::str::FromStr;

use uv_small_str::SmallString;

/// An invalid wheel variant label.
#[derive(Debug, thiserror::Error)]
pub enum InvalidVariantLabel {
    #[error("must not be empty")]
    Empty,
    #[error("must contain only lowercase ASCII letters, digits, underscores, and periods")]
    InvalidCharacters,
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
    rkyv::Archive,
    rkyv::Deserialize,
    rkyv::Serialize,
)]
#[rkyv(derive(Debug))]
pub struct VariantLabel(SmallString);

impl FromStr for VariantLabel {
    type Err = InvalidVariantLabel;

    fn from_str(label: &str) -> Result<Self, Self::Err> {
        if label.is_empty() {
            return Err(InvalidVariantLabel::Empty);
        }
        if !label.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_')
        }) {
            return Err(InvalidVariantLabel::InvalidCharacters);
        }

        Ok(Self(SmallString::from(label)))
    }
}

impl Display for VariantLabel {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.0, f)
    }
}
