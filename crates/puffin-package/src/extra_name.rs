use std::fmt;
use std::fmt::{Display, Formatter};
use std::str::FromStr;

use anyhow::{anyhow, Error, Result};
use once_cell::sync::Lazy;
use regex::Regex;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ExtraName(String);

impl Display for ExtraName {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

static NAME_NORMALIZE: Lazy<Regex> = Lazy::new(|| Regex::new(r"[-_.]+").unwrap());
static NAME_VALIDATE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)^([A-Z0-9]|[A-Z0-9][A-Z0-9._-]*[A-Z0-9])$").unwrap());

/// An extra dependency group name.
///
/// See:
/// - <https://peps.python.org/pep-0685/#specification/>
/// - <https://packaging.python.org/en/latest/specifications/name-normalization/>
impl ExtraName {
    /// Create a normalized extra name without validation.
    ///
    /// Collapses any run of the characters `-`, `_` and `.` down to a single `-`.
    /// Ex) "---", ".", and "__" all get converted to just "."
    pub fn normalize(name: impl AsRef<str>) -> Self {
        // TODO(charlie): Avoid allocating in the common case (when no normalization is required).
        let mut normalized = NAME_NORMALIZE.replace_all(name.as_ref(), "-").to_string();
        normalized.make_ascii_lowercase();
        Self(normalized)
    }

    /// Create a validated, normalized extra name.
    pub fn validate(name: impl AsRef<str>) -> Result<Self> {
        if NAME_VALIDATE.is_match(name.as_ref()) {
            Ok(Self::normalize(name))
        } else {
            Err(anyhow!(
                "Extra names must start and end with a letter or digit and may only contain -, _, ., and alphanumeric characters"
            ))
        }
    }
}

impl AsRef<str> for ExtraName {
    fn as_ref(&self) -> &str {
        self.0.as_ref()
    }
}

impl FromStr for ExtraName {
    type Err = Error;

    fn from_str(name: &str) -> Result<Self> {
        Self::validate(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize() {
        assert_eq!(
            ExtraName::normalize("friendly-bard").as_ref(),
            "friendly-bard"
        );
        assert_eq!(
            ExtraName::normalize("Friendly-Bard").as_ref(),
            "friendly-bard"
        );
        assert_eq!(
            ExtraName::normalize("FRIENDLY-BARD").as_ref(),
            "friendly-bard"
        );
        assert_eq!(
            ExtraName::normalize("friendly.bard").as_ref(),
            "friendly-bard"
        );
        assert_eq!(
            ExtraName::normalize("friendly_bard").as_ref(),
            "friendly-bard"
        );
        assert_eq!(
            ExtraName::normalize("friendly--bard").as_ref(),
            "friendly-bard"
        );
        assert_eq!(
            ExtraName::normalize("FrIeNdLy-._.-bArD").as_ref(),
            "friendly-bard"
        );
    }

    #[test]
    fn validate() {
        // Unchanged
        assert_eq!(
            ExtraName::validate("friendly-bard").unwrap().as_ref(),
            "friendly-bard"
        );
        assert_eq!(ExtraName::validate("1okay").unwrap().as_ref(), "1okay");
        assert_eq!(ExtraName::validate("okay2").unwrap().as_ref(), "okay2");
        // Normalizes
        assert_eq!(
            ExtraName::validate("Friendly-Bard").unwrap().as_ref(),
            "friendly-bard"
        );
        // Failures...
        assert!(ExtraName::validate(" starts-with-space").is_err());
        assert!(ExtraName::validate("-starts-with-dash").is_err());
        assert!(ExtraName::validate("ends-with-dash-").is_err());
        assert!(ExtraName::validate("ends-with-space ").is_err());
        assert!(ExtraName::validate("includes!invalid-char").is_err());
        assert!(ExtraName::validate("space in middle").is_err());
    }
}
