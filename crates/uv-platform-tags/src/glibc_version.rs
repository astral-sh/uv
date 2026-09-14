use std::fmt::{Display, Formatter};
use std::str::FromStr;

/// A glibc version, expressed as a major and minor release (e.g., `2.31`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GlibcVersion {
    major: u16,
    minor: u16,
}

impl GlibcVersion {
    /// Create a glibc version from its major and minor release numbers.
    pub const fn new(major: u16, minor: u16) -> Self {
        Self { major, minor }
    }
}

impl FromStr for GlibcVersion {
    type Err = ParseGlibcVersionError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let (major, minor) = value.split_once('.').ok_or(ParseGlibcVersionError)?;
        if !major.bytes().all(|byte| byte.is_ascii_digit())
            || !minor.bytes().all(|byte| byte.is_ascii_digit())
        {
            return Err(ParseGlibcVersionError);
        }
        Ok(Self {
            major: major.parse().map_err(|_| ParseGlibcVersionError)?,
            minor: minor.parse().map_err(|_| ParseGlibcVersionError)?,
        })
    }
}

impl Display for GlibcVersion {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}.{}", self.major, self.minor)
    }
}

impl serde::Serialize for GlibcVersion {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> serde::Deserialize<'de> for GlibcVersion {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;

        impl serde::de::Visitor<'_> for Visitor {
            type Value = GlibcVersion;

            fn expecting(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a glibc version string in the form `<major>.<minor>`")
            }

            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                value.parse().map_err(E::custom)
            }
        }

        deserializer.deserialize_str(Visitor)
    }
}

#[derive(Debug, thiserror::Error)]
#[error("expected a glibc version in the form `<major>.<minor>` (e.g., `2.31`)")]
pub struct ParseGlibcVersionError;

#[cfg(test)]
mod tests {
    use super::GlibcVersion;

    #[test]
    fn parse() -> Result<(), super::ParseGlibcVersionError> {
        let version = "2.31".parse::<GlibcVersion>()?;
        assert_eq!(version, GlibcVersion::new(2, 31));
        assert_eq!(version.to_string(), "2.31");
        assert!(version > "2.9".parse()?);
        assert!(version < "3.0".parse()?);

        for value in [
            "", "2", "2.", ".31", "2.31.0", ">=2.31", "+2.31", "2.-1", "2.65536",
        ] {
            assert!(value.parse::<GlibcVersion>().is_err(), "{value}");
        }
        Ok(())
    }
}
