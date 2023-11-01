use std::fmt;
use std::fmt::{Display, Formatter};

use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Deserializer, Serialize};

/// The normalized name of a package.
///
/// Converts the name to lowercase and collapses any run of the characters `-`, `_` and `.`
/// down to a single `-`, e.g., `---`, `.`, and `__` all get converted to just `-`.
///
/// See: <https://packaging.python.org/en/latest/specifications/name-normalization/>
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
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

impl PackageName {
    pub fn new(name: impl AsRef<str>) -> Self {
        // TODO(charlie): Avoid allocating in the common case (when no normalization is required).
        let mut normalized = NAME_NORMALIZE.replace_all(name.as_ref(), "-").to_string();
        normalized.make_ascii_lowercase();
        Self(normalized)
    }
}

impl AsRef<str> for PackageName {
    fn as_ref(&self) -> &str {
        self.0.as_ref()
    }
}

impl<'de> Deserialize<'de> for PackageName {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(Self::new(s))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize() {
        assert_eq!(PackageName::new("friendly-bard").as_ref(), "friendly-bard");
        assert_eq!(PackageName::new("Friendly-Bard").as_ref(), "friendly-bard");
        assert_eq!(PackageName::new("FRIENDLY-BARD").as_ref(), "friendly-bard");
        assert_eq!(PackageName::new("friendly.bard").as_ref(), "friendly-bard");
        assert_eq!(PackageName::new("friendly_bard").as_ref(), "friendly-bard");
        assert_eq!(PackageName::new("friendly--bard").as_ref(), "friendly-bard");
        assert_eq!(
            PackageName::new("FrIeNdLy-._.-bArD").as_ref(),
            "friendly-bard"
        );
    }
}
