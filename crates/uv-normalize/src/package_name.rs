use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::hash::{BuildHasherDefault, Hasher};
use std::str::FromStr;

use byteorder::{ByteOrder, NativeEndian};
use serde::{Deserialize, Deserializer, Serialize};

use crate::string::InternedString;
use crate::{validate_and_normalize_owned, validate_and_normalize_ref, InvalidNameError};

/// The normalized name of a package.
///
/// Converts the name to lowercase and collapses runs of `-`, `_`, and `.` down to a single `-`.
/// For example, `---`, `.`, and `__` are all converted to a single `-`.
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
pub struct PackageName(InternedString);

impl PackageName {
    /// Create a validated, normalized package name.
    pub fn new(name: String) -> Result<Self, InvalidNameError> {
        validate_and_normalize_owned(name)
            .map(InternedString::from)
            .map(Self)
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

    /// Returns the underlying package name.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&Self> for PackageName {
    /// Required for `WaitMap::wait`.
    fn from(package_name: &Self) -> Self {
        package_name.clone()
    }
}

impl FromStr for PackageName {
    type Err = InvalidNameError;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        validate_and_normalize_ref(name)
            .map(InternedString::from)
            .map(Self)
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

#[cfg(feature = "schemars")]
impl schemars::JsonSchema for PackageName {
    fn schema_name() -> String {
        <String as schemars::JsonSchema>::schema_name()
    }

    fn json_schema(gen: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
        <String as schemars::JsonSchema>::json_schema(gen)
    }
}

pub type InternedMap<K, V> = HashMap<K, V, BuildHasherDefault<IdentityHasher>>;

pub type InternedSet<K> = HashSet<K, BuildHasherDefault<IdentityHasher>>;

#[derive(Default)]
pub struct IdentityHasher {
    hash: u64,
}

impl Hasher for IdentityHasher {
    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        if bytes.len() == 8 {
            self.hash = NativeEndian::read_u64(bytes);
        }
    }

    #[inline]
    fn finish(&self) -> u64 {
        self.hash
    }
}
