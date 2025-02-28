use std::ops::Deref;
use std::str::FromStr;

use thiserror::Error;

/// The normalized name of an index.
///
/// Index names may contain letters, digits, hyphens, underscores, and periods, and must be ASCII.
#[derive(Debug, Clone, Hash, Eq, PartialEq, serde::Serialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct IndexName(String);

impl IndexName {
    /// Validates the given index name and returns [`IndexName`] if it's valid, or an error
    /// otherwise.
    pub fn new(name: String) -> Result<Self, IndexNameError> {
        for c in name.chars() {
            match c {
                'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' => {}
                c if c.is_ascii() => {
                    return Err(IndexNameError::UnsupportedCharacter(c, name));
                }
                c => {
                    return Err(IndexNameError::NonAsciiName(c, name));
                }
            }
        }
        Ok(Self(name))
    }

    /// Converts the index name to an environment variable name.
    ///
    /// For example, given `IndexName("foo-bar")`, this will return `"FOO_BAR"`.
    pub fn to_env_var(&self) -> String {
        self.0
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() {
                    c.to_ascii_uppercase()
                } else {
                    '_'
                }
            })
            .collect::<String>()
    }
}

impl FromStr for IndexName {
    type Err = IndexNameError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s.to_string())
    }
}

impl<'de> serde::de::Deserialize<'de> for IndexName {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        IndexName::new(s).map_err(serde::de::Error::custom)
    }
}

impl std::fmt::Display for IndexName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl AsRef<str> for IndexName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl Deref for IndexName {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// An error that can occur when parsing an [`IndexName`].
#[derive(Error, Debug)]
pub enum IndexNameError {
    #[error("Index included a name, but the name was empty")]
    EmptyName,
    #[error("Index names may only contain letters, digits, hyphens, underscores, and periods, but found unsupported character (`{0}`) in: `{1}`")]
    UnsupportedCharacter(char, String),
    #[error("Index names must be ASCII, but found non-ASCII character (`{0}`) in: `{1}`")]
    NonAsciiName(char, String),
}
