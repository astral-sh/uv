use std::fmt;
use std::fmt::{Display, Formatter};
use std::ops::Deref;

use once_cell::sync::Lazy;
use regex::Regex;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PackageName(String);

impl Display for PackageName {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

static NAME_NORMALIZE: Lazy<Regex> = Lazy::new(|| Regex::new(r"[-_.]").unwrap());

impl PackageName {
    /// See: <https://packaging.python.org/en/latest/specifications/name-normalization/>
    pub fn normalize(name: impl AsRef<str>) -> Self {
        // TODO(charlie): Avoid allocating in the common case (when no normalization is required).
        let mut normalized = NAME_NORMALIZE.replace_all(name.as_ref(), "-").to_string();
        normalized.make_ascii_lowercase();
        Self(normalized)
    }
}

impl Deref for PackageName {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
