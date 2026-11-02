use std::fmt::{Display, Formatter};
use std::str::FromStr;

/// A libc version, expressed as a major and minor release (e.g., `2.31` or `1.2`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LibcVersion {
    major: u16,
    minor: u16,
}

impl LibcVersion {
    pub const fn new(major: u16, minor: u16) -> Self {
        Self { major, minor }
    }
}

impl FromStr for LibcVersion {
    type Err = ParseLibcVersionError;

    /// Parse exactly two unsigned decimal components that fit in [`u16`].
    ///
    /// Signs, whitespace, and additional release components are rejected. Leading zeroes are
    /// accepted and normalized by [`Display`].
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let (major, minor) = value.split_once('.').ok_or(ParseLibcVersionError)?;
        if !major.bytes().all(|byte| byte.is_ascii_digit())
            || !minor.bytes().all(|byte| byte.is_ascii_digit())
        {
            return Err(ParseLibcVersionError);
        }
        Ok(Self {
            major: major.parse().map_err(|_| ParseLibcVersionError)?,
            minor: minor.parse().map_err(|_| ParseLibcVersionError)?,
        })
    }
}

impl Display for LibcVersion {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}.{}", self.major, self.minor)
    }
}

impl serde::Serialize for LibcVersion {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> serde::Deserialize<'de> for LibcVersion {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;

        impl serde::de::Visitor<'_> for Visitor {
            type Value = LibcVersion;

            fn expecting(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a libc version string in the form `<major>.<minor>`")
            }

            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                value.parse().map_err(E::custom)
            }
        }

        deserializer.deserialize_str(Visitor)
    }
}

#[derive(Debug, thiserror::Error)]
#[error("expected a libc version in the form `<major>.<minor>` (e.g., `2.31` or `1.2`)")]
pub struct ParseLibcVersionError;

#[cfg(test)]
mod tests {
    use super::LibcVersion;

    #[test]
    fn parse() -> Result<(), super::ParseLibcVersionError> {
        let version = "2.31".parse::<LibcVersion>()?;
        assert_eq!(version, LibcVersion::new(2, 31));
        assert_eq!(version.to_string(), "2.31");
        assert!(version > "2.9".parse()?);
        assert!(version < "3.0".parse()?);

        for value in [
            "", "2", "2.", ".31", "2.31.0", ">=2.31", "+2.31", "2.-1", "2.65536",
        ] {
            assert!(value.parse::<LibcVersion>().is_err(), "{value}");
        }
        Ok(())
    }
}
