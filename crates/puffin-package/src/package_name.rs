use std::fmt;
use std::fmt::{Display, Formatter};

use crate::dist_info_name::DistInfoName;
use anyhow::{anyhow, Result};
use once_cell::sync::Lazy;
use regex::Regex;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PackageName(String);

impl From<&PackageName> for PackageName {
    /// Required for `WaitMap::wait`
    fn from(package_name: &PackageName) -> Self {
        package_name.clone()
    }
}

impl Display for PackageName {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

static NAME_NORMALIZE: Lazy<Regex> = Lazy::new(|| Regex::new(r"[-_.]+").unwrap());
static NAME_VALIDATE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)^([A-Z0-9]|[A-Z0-9][A-Z0-9._-]*[A-Z0-9])$").unwrap());

/// A package name.
///
/// See:
/// - <https://packaging.python.org/en/latest/specifications/name-normalization/>
/// - <https://peps.python.org/pep-0508/#names>
/// - <https://peps.python.org/pep-0503/#normalized-names>
impl PackageName {
    /// Create a normalized package name without validation.
    ///
    /// Converts the name to lowercase and collapses any run of the characters `-`, `_` and `.`
    /// down to a single `-`, e.g., `---`, `.`, and `__` all get converted to just `-`.
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
                "Package names must start and end with a letter or digit and may only contain -, _, ., and alphanumeric characters"
            ))
        }
    }
}

impl AsRef<str> for PackageName {
    fn as_ref(&self) -> &str {
        self.0.as_ref()
    }
}

impl From<DistInfoName> for PackageName {
    fn from(dist_info_name: DistInfoName) -> Self {
        Self::normalize(dist_info_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize() {
        assert_eq!(
            PackageName::normalize("friendly-bard").as_ref(),
            "friendly-bard"
        );
        assert_eq!(
            PackageName::normalize("Friendly-Bard").as_ref(),
            "friendly-bard"
        );
        assert_eq!(
            PackageName::normalize("FRIENDLY-BARD").as_ref(),
            "friendly-bard"
        );
        assert_eq!(
            PackageName::normalize("friendly.bard").as_ref(),
            "friendly-bard"
        );
        assert_eq!(
            PackageName::normalize("friendly_bard").as_ref(),
            "friendly-bard"
        );
        assert_eq!(
            PackageName::normalize("friendly--bard").as_ref(),
            "friendly-bard"
        );
        assert_eq!(
            PackageName::normalize("FrIeNdLy-._.-bArD").as_ref(),
            "friendly-bard"
        );
    }

    #[test]
    fn validate() {
        // Unchanged
        assert_eq!(
            PackageName::validate("friendly-bard").unwrap().as_ref(),
            "friendly-bard"
        );
        assert_eq!(PackageName::validate("1okay").unwrap().as_ref(), "1okay");
        assert_eq!(PackageName::validate("okay2").unwrap().as_ref(), "okay2");
        // Normalizes
        assert_eq!(
            PackageName::validate("Friendly-Bard").unwrap().as_ref(),
            "friendly-bard"
        );
        // Failures...
        assert!(PackageName::validate(" starts-with-space").is_err());
        assert!(PackageName::validate("-starts-with-dash").is_err());
        assert!(PackageName::validate("ends-with-dash-").is_err());
        assert!(PackageName::validate("ends-with-space ").is_err());
        assert!(PackageName::validate("includes!invalid-char").is_err());
        assert!(PackageName::validate("space in middle").is_err());
    }
}
