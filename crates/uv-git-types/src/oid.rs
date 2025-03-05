use std::fmt::{Display, Formatter};
use std::str::{self, FromStr};

use thiserror::Error;

/// Unique identity of any Git object (commit, tree, blob, tag).
///
/// Note this type does not validate whether the input is a valid hash.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GitOid {
    len: usize,
    bytes: [u8; 40],
}

impl GitOid {
    /// Return the string representation of an object ID.
    pub fn as_str(&self) -> &str {
        str::from_utf8(&self.bytes[..self.len]).unwrap()
    }

    /// Return a truncated representation, i.e., the first 16 characters of the SHA.
    pub fn as_short_str(&self) -> &str {
        &self.as_str()[..16]
    }
}

#[derive(Debug, Error, PartialEq)]
pub enum OidParseError {
    #[error("Object ID can be at most 40 hex characters")]
    TooLong,
    #[error("Object ID cannot be parsed from empty string")]
    Empty,
}

impl FromStr for GitOid {
    type Err = OidParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() {
            return Err(OidParseError::Empty);
        }

        if s.len() > 40 {
            return Err(OidParseError::TooLong);
        }

        let mut out = [0; 40];
        out[..s.len()].copy_from_slice(s.as_bytes());

        Ok(GitOid {
            len: s.len(),
            bytes: out,
        })
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

        assert_eq!(GitOid::from_str(""), Err(OidParseError::Empty));
        assert_eq!(
            GitOid::from_str(&str::repeat("a", 41)),
            Err(OidParseError::TooLong)
        );
    }
}
