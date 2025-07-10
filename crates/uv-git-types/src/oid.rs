use std::fmt::{Display, Formatter};
use std::str::{self, FromStr};

use thiserror::Error;

/// Unique identity of any Git object (commit, tree, blob, tag).
///
/// This type's `FromStr` implementation validates that it's exactly 40 hex characters, i.e. a
/// full-length git commit.
///
/// If Git's SHA-256 support becomes more widespread in the future (in particular if GitHub ever
/// adds support), we might need to make this an enum.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GitOid {
    bytes: [u8; 40],
}

impl GitOid {
    /// Return the string representation of an object ID.
    pub fn as_str(&self) -> &str {
        str::from_utf8(&self.bytes).unwrap()
    }

    /// Return a truncated representation, i.e., the first 16 characters of the SHA.
    pub fn as_short_str(&self) -> &str {
        &self.as_str()[..16]
    }
}

#[derive(Debug, Error, PartialEq)]
pub enum OidParseError {
    #[error("Object ID cannot be parsed from empty string")]
    Empty,
    #[error("Object ID must be exactly 40 hex characters")]
    WrongLength,
    #[error("Object ID must be valid hex characters")]
    NotHex,
}

impl FromStr for GitOid {
    type Err = OidParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() {
            return Err(OidParseError::Empty);
        }

        if s.len() != 40 {
            return Err(OidParseError::WrongLength);
        }

        if !s.chars().all(|ch| ch.is_ascii_hexdigit()) {
            return Err(OidParseError::NotHex);
        }

        let mut bytes = [0; 40];
        bytes.copy_from_slice(s.as_bytes());
        Ok(GitOid { bytes })
    }
}

impl Display for GitOid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl serde::Serialize for GitOid {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.as_str().serialize(serializer)
    }
}

impl<'de> serde::Deserialize<'de> for GitOid {
    fn deserialize<D>(deserializer: D) -> Result<GitOid, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct Visitor;

        impl serde::de::Visitor<'_> for Visitor {
            type Value = GitOid;

            fn expecting(&self, f: &mut Formatter) -> std::fmt::Result {
                f.write_str("a string")
            }

            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
                GitOid::from_str(v).map_err(serde::de::Error::custom)
            }
        }

        deserializer.deserialize_str(Visitor)
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::{GitOid, OidParseError};

    #[test]
    fn git_oid() {
        GitOid::from_str("4a23745badf5bf5ef7928f1e346e9986bd696d82").unwrap();
        GitOid::from_str("4A23745BADF5BF5EF7928F1E346E9986BD696D82").unwrap();

        assert_eq!(GitOid::from_str(""), Err(OidParseError::Empty));
        assert_eq!(
            GitOid::from_str(&str::repeat("a", 41)),
            Err(OidParseError::WrongLength)
        );
        assert_eq!(
            GitOid::from_str(&str::repeat("a", 39)),
            Err(OidParseError::WrongLength)
        );
        assert_eq!(
            GitOid::from_str(&str::repeat("x", 40)),
            Err(OidParseError::NotHex)
        );
    }
}
