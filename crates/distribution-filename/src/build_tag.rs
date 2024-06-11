use std::num::ParseIntError;
use std::str::FromStr;
use std::sync::Arc;

#[derive(thiserror::Error, Debug)]
pub enum BuildTagError {
    #[error("must not be empty")]
    Empty,
    #[error("must start with a digit")]
    NoLeadingDigit,
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
#[archive(check_bytes)]
#[archive_attr(derive(Debug))]
pub struct BuildTag(u64, Option<Arc<str>>);

impl FromStr for BuildTag {
    type Err = BuildTagError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // A build tag must not be empty.
        if s.is_empty() {
            return Err(BuildTagError::Empty);
        }

        // A build tag must start with a digit.
        let (prefix, suffix) = match s.find(|c: char| !c.is_ascii_digit()) {
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

        Ok(BuildTag(prefix.parse::<u64>()?, suffix.map(Arc::from)))
    }
}
