use std::num::ParseIntError;
use std::str::FromStr;

use uv_small_str::SmallString;

#[derive(thiserror::Error, Debug)]
pub enum BuildTagError {
    #[error("must not be empty")]
    Empty,
    #[error("must start with a digit")]
    NoLeadingDigit,
    #[error("must contain only ASCII letters, digits, underscores, and periods")]
    InvalidCharacters,
    #[error(transparent)]
    ParseInt(#[from] ParseIntError),
}

/// The optional build tag for a wheel:
///
/// > Must start with a digit. Acts as a tie-breaker if two wheel file names are the same in all
/// > other respects (i.e. name, version, and other tags). Sort as an empty tuple if unspecified,
/// > else sort as a two-item tuple with the first item being the initial digits as an int, and the
/// > second item being the remainder of the tag as a str.
///
/// See: <https://packaging.python.org/en/latest/specifications/binary-distribution-format/#file-name-convention>
#[derive(
    Debug,
    Clone,
    Eq,
    PartialEq,
    Hash,
    Ord,
    PartialOrd,
    rkyv::Archive,
    rkyv::Deserialize,
    rkyv::Serialize,
)]
#[rkyv(derive(Debug))]
pub struct BuildTag(u64, Option<SmallString>);

impl FromStr for BuildTag {
    type Err = BuildTagError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // A build tag must not be empty.
        if s.is_empty() {
            return Err(BuildTagError::Empty);
        }

        let mut prefix_end = None;
        for (index, byte) in s.bytes().enumerate() {
            if !is_build_tag_byte(byte) {
                return Err(BuildTagError::InvalidCharacters);
            }
            if prefix_end.is_none() && !byte.is_ascii_digit() {
                prefix_end = Some(index);
            }
        }

        // A build tag must start with a digit.
        let (prefix, suffix) = match prefix_end {
            // Ex) `abc`
            Some(0) => return Err(BuildTagError::NoLeadingDigit),
            // Ex) `123abc`
            Some(split) => {
                let (prefix, suffix) = s.split_at(split);
                (prefix, Some(suffix))
            }
            // Ex) `123`
            None => (s, None),
        };

        Ok(Self(prefix.parse::<u64>()?, suffix.map(SmallString::from)))
    }
}

impl std::fmt::Display for BuildTag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.1 {
            Some(suffix) => write!(f, "{}{}", self.0, suffix),
            None => write!(f, "{}", self.0),
        }
    }
}

fn is_build_tag_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.')
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::BuildTag;

    #[test]
    fn parse_periods() {
        assert_eq!(
            BuildTag::from_str("0.editable")
                .map(|build_tag| build_tag.to_string())
                .map_err(|err| err.to_string()),
            Ok("0.editable".to_string())
        );
    }

    #[test]
    fn err_invalid_characters() {
        let err = BuildTag::from_str("1/../../target").unwrap_err();
        insta::assert_snapshot!(err, @"must contain only ASCII letters, digits, underscores, and periods");

        let err = BuildTag::from_str(r"1..\..\target").unwrap_err();
        insta::assert_snapshot!(err, @"must contain only ASCII letters, digits, underscores, and periods");

        let err = BuildTag::from_str("1target:stream").unwrap_err();
        insta::assert_snapshot!(err, @"must contain only ASCII letters, digits, underscores, and periods");

        let err = BuildTag::from_str("1-target").unwrap_err();
        insta::assert_snapshot!(err, @"must contain only ASCII letters, digits, underscores, and periods");

        let err = BuildTag::from_str("1 target").unwrap_err();
        insta::assert_snapshot!(err, @"must contain only ASCII letters, digits, underscores, and periods");

        let err = BuildTag::from_str("1target\u{e9}").unwrap_err();
        insta::assert_snapshot!(err, @"must contain only ASCII letters, digits, underscores, and periods");
    }
}
