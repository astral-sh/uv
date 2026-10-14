use std::fmt::{Debug, Display, Formatter};
use std::str::{self, FromStr};

use thiserror::Error;

const SHA1_HEX_LENGTH: usize = 40;
const SHA256_HEX_LENGTH: usize = 64;

/// The hash algorithm used to identify objects in a Git repository.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum GitObjectFormat {
    Sha1,
    Sha256,
}

impl GitObjectFormat {
    /// Return the name Git uses for this object format.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sha1 => "sha1",
            Self::Sha256 => "sha256",
        }
    }

    pub(crate) const fn hex_length(self) -> usize {
        match self {
            Self::Sha1 => SHA1_HEX_LENGTH,
            Self::Sha256 => SHA256_HEX_LENGTH,
        }
    }
}

/// Unique identity of any Git object (commit, tree, blob, tag).
///
/// This type's `FromStr` implementation validates a full-length SHA-1 or SHA-256 object ID and
/// normalizes it to lowercase.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GitOid {
    bytes: [u8; SHA256_HEX_LENGTH],
    format: GitObjectFormat,
}

impl GitOid {
    /// Return the string representation of an object ID.
    pub fn as_str(&self) -> &str {
        str::from_utf8(&self.bytes[..self.format.hex_length()])
            .expect("Git object IDs contain only hexadecimal characters")
    }

    /// Return the hash algorithm used to identify this object.
    pub const fn object_format(&self) -> GitObjectFormat {
        self.format
    }

    /// Return a truncated representation, i.e., the first 16 characters of the SHA.
    pub fn as_short_str(&self) -> &str {
        &self.as_str()[..16]
    }

    /// Return a (very) truncated representation, i.e., the first 8 characters of the SHA.
    pub fn as_tiny_str(&self) -> &str {
        &self.as_str()[..8]
    }
}

impl Debug for GitOid {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "GitOid(\"{}\")", self.as_str())
    }
}

#[derive(Debug, Error, PartialEq)]
pub enum OidParseError {
    #[error("Object ID cannot be parsed from empty string")]
    Empty,
    #[error("Object ID must be exactly 40 or 64 hex characters")]
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

        let format = match s.len() {
            SHA1_HEX_LENGTH => GitObjectFormat::Sha1,
            SHA256_HEX_LENGTH => GitObjectFormat::Sha256,
            _ => return Err(OidParseError::WrongLength),
        };

        if !s.chars().all(|ch| ch.is_ascii_hexdigit()) {
            return Err(OidParseError::NotHex);
        }

        let mut bytes = [0; SHA256_HEX_LENGTH];
        bytes[..s.len()].copy_from_slice(s.as_bytes());
        bytes.make_ascii_lowercase();
        Ok(Self { bytes, format })
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
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
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

    use super::{GitObjectFormat, GitOid, OidParseError};

    #[test]
    fn git_oid() -> Result<(), OidParseError> {
        assert_eq!(
            GitOid::from_str("4a23745badf5bf5ef7928f1e346e9986bd696d82")?,
            GitOid::from_str("4A23745BADF5BF5EF7928F1E346E9986BD696D82")?
        );
        let sha256 = GitOid::from_str(&str::repeat("A", 64))?;
        assert_eq!(sha256.as_str(), str::repeat("a", 64));
        assert_eq!(sha256.object_format(), GitObjectFormat::Sha256);

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

        Ok(())
    }
}
