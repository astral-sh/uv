use std::fmt;
use std::fmt::{Display, Formatter};

use once_cell::sync::Lazy;
use regex::Regex;

use crate::dist_info_name::DistInfoName;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PackageName(String);

impl From<&PackageName> for PackageName {
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
    /// See: <https://packaging.python.org/en/latest/specifications/name-normalization/>
    pub fn normalize(name: impl AsRef<str>) -> Self {
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
}
