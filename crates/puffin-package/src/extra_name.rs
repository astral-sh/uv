use std::fmt;
use std::fmt::{Display, Formatter};

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

impl ExtraName {
    /// Collapses any run of the characters `-`, `_` and `.` down to a single `-`.
    /// Ex) "---", ".", and "__" all get converted to just "."
    ///
    /// See: <https://peps.python.org/pep-0685/#specification/>
    ///      <https://packaging.python.org/en/latest/specifications/name-normalization/>
    pub fn normalize(name: impl AsRef<str>) -> Self {
        // TODO(charlie): Avoid allocating in the common case (when no normalization is required).
        let mut normalized = NAME_NORMALIZE.replace_all(name.as_ref(), "-").to_string();
        normalized.make_ascii_lowercase();
        Self(normalized)
    }
}

impl AsRef<str> for ExtraName {
    fn as_ref(&self) -> &str {
        self.0.as_ref()
    }
}

impl From<&str> for ExtraName {
    fn from(name: &str) -> Self {
        Self::normalize(name)
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
}
