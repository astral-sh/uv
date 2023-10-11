use std::fmt;
use std::fmt::{Display, Formatter};

use once_cell::sync::Lazy;
use regex::Regex;

use crate::package_name::PackageName;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DistInfoName(String);

impl Display for DistInfoName {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

static NAME_NORMALIZE: Lazy<Regex> = Lazy::new(|| Regex::new(r"[-_.]+").unwrap());

impl DistInfoName {
    /// See: <https://packaging.python.org/en/latest/specifications/recording-installed-packages/#recording-installed-packages>
    pub fn normalize(name: impl AsRef<str>) -> Self {
        // TODO(charlie): Avoid allocating in the common case (when no normalization is required).
        let mut normalized = NAME_NORMALIZE.replace_all(name.as_ref(), "_").to_string();
        normalized.make_ascii_lowercase();
        Self(normalized)
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl AsRef<str> for DistInfoName {
    fn as_ref(&self) -> &str {
        self.0.as_ref()
    }
}

impl From<&PackageName> for DistInfoName {
    fn from(package_name: &PackageName) -> Self {
        Self::normalize(package_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize() {
        assert_eq!(
            DistInfoName::normalize("friendly-bard").as_ref(),
            "friendly_bard"
        );
        assert_eq!(
            DistInfoName::normalize("Friendly-Bard").as_ref(),
            "friendly_bard"
        );
        assert_eq!(
            DistInfoName::normalize("FRIENDLY-BARD").as_ref(),
            "friendly_bard"
        );
        assert_eq!(
            DistInfoName::normalize("friendly.bard").as_ref(),
            "friendly_bard"
        );
        assert_eq!(
            DistInfoName::normalize("friendly_bard").as_ref(),
            "friendly_bard"
        );
        assert_eq!(
            DistInfoName::normalize("friendly--bard").as_ref(),
            "friendly_bard"
        );
        assert_eq!(
            DistInfoName::normalize("FrIeNdLy-._.-bArD").as_ref(),
            "friendly_bard"
        );
    }
}
