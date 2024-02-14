use std::borrow::Cow;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize};

use crate::{validate_and_normalize_owned, validate_and_normalize_ref, InvalidNameError};

/// The normalized name of a package.
///
/// Converts the name to lowercase and collapses any run of the characters `-`, `_` and `.`
/// down to a single `-`, e.g., `---`, `.`, and `__` all get converted to just `-`.
///
/// See: <https://packaging.python.org/en/latest/specifications/name-normalization/>
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    Serialize,
    rkyv::Archive,
    rkyv::Deserialize,
    rkyv::Serialize,
)]
#[archive(check_bytes)]
#[archive_attr(derive(Debug))]
pub struct PackageName(String);

impl PackageName {
    /// Create a validated, normalized package name.
    pub fn new(name: String) -> Result<Self, InvalidNameError> {
        validate_and_normalize_owned(name).map(Self)
    }

    /// Escape this name with underscores (`_`) instead of dashes (`-`)
    ///
    /// See: <https://packaging.python.org/en/latest/specifications/recording-installed-packages/#recording-installed-packages>
    pub fn as_dist_info_name(&self) -> Cow<'_, str> {
        if let Some(dash_position) = self.0.find('-') {
            // Initialize `replaced` with the start of the string up to the current character.
            let mut owned_string = String::with_capacity(self.0.len());
            owned_string.push_str(&self.0[..dash_position]);
            owned_string.push('_');

            // Iterate over the rest of the string.
            owned_string.extend(self.0[dash_position + 1..].chars().map(|character| {
                if character == '-' {
                    '_'
                } else {
                    character
                }
            }));

            Cow::Owned(owned_string)
        } else {
            Cow::Borrowed(self.0.as_str())
        }
    }
}

impl From<&PackageName> for PackageName {
    /// Required for `WaitMap::wait`.
    fn from(package_name: &PackageName) -> Self {
        package_name.clone()
    }
}

impl FromStr for PackageName {
    type Err = InvalidNameError;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        validate_and_normalize_ref(name).map(Self)
    }
}

impl<'de> Deserialize<'de> for PackageName {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::from_str(&s).map_err(serde::de::Error::custom)
    }
}

impl std::fmt::Display for PackageName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl AsRef<str> for PackageName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}
