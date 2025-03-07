use std::fmt::Display;
use std::str::FromStr;
use thiserror::Error;

/// Simplified Python identifier.
///
/// We don't match Python's identifier rules
/// (<https://docs.python.org/3.13/reference/lexical_analysis.html#identifiers>) exactly
/// (we just use Rust's `is_alphabetic`) and we don't convert to NFKC, but it's good enough
/// for our validation purposes.
#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct Identifier(Box<str>);

#[derive(Debug, Clone, Error)]
pub enum IdentifierParseError {
    #[error("An identifier must not be empty")]
    Empty,
    #[error(
        "Invalid first character `{first}` for identifier `{identifier}`, expected an underscore or an alphabetic character"
    )]
    InvalidFirstChar { first: char, identifier: Box<str> },
    #[error(
        "Invalid character `{invalid_char}` at position {pos} for identifier `{identifier}`, \
        expected an underscore or an alphanumeric character"
    )]
    InvalidChar {
        pos: usize,
        invalid_char: char,
        identifier: Box<str>,
    },
}

impl Identifier {
    pub fn new(identifier: impl Into<Box<str>>) -> Result<Self, IdentifierParseError> {
        let identifier = identifier.into();
        let mut chars = identifier.chars().enumerate();
        let (_, first_char) = chars.next().ok_or(IdentifierParseError::Empty)?;
        if first_char != '_' && !first_char.is_alphabetic() {
            return Err(IdentifierParseError::InvalidFirstChar {
                first: first_char,
                identifier,
            });
        }

        for (pos, current_char) in chars {
            if current_char != '_' && !current_char.is_alphanumeric() {
                return Err(IdentifierParseError::InvalidChar {
                    // Make the position 1-indexed
                    pos: pos + 1,
                    invalid_char: current_char,
                    identifier,
                });
            }
        }

        Ok(Self(identifier))
    }
}

impl FromStr for Identifier {
    type Err = IdentifierParseError;

    fn from_str(identifier: &str) -> Result<Self, Self::Err> {
        Self::new(identifier.to_string())
    }
}

impl Display for Identifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl AsRef<str> for Identifier {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl<'de> serde::de::Deserialize<'de> for Identifier {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Identifier::from_str(&s).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use insta::assert_snapshot;

    #[test]
    fn valid() {
        let valid_ids = vec![
            "abc",
            "_abc",
            "a_bc",
            "a123",
            "snake_case",
            "camelCase",
            "PascalCase",
            // A single character is valid
            "_",
            "a",
            // Unicode
            "α",
            "férrîs",
            "안녕하세요",
        ];

        for valid_id in valid_ids {
            assert!(Identifier::from_str(valid_id).is_ok(), "{}", valid_id);
        }
    }

    #[test]
    fn empty() {
        assert_snapshot!(Identifier::from_str("").unwrap_err(), @"An identifier must not be empty");
    }

    #[test]
    fn invalid_first_char() {
        assert_snapshot!(
            Identifier::from_str("1foo").unwrap_err(),
            @"Invalid first character `1` for identifier `1foo`, expected an underscore or an alphabetic character"
        );
        assert_snapshot!(
            Identifier::from_str("$foo").unwrap_err(),
            @"Invalid first character `$` for identifier `$foo`, expected an underscore or an alphabetic character"
        );
        assert_snapshot!(
            Identifier::from_str(".foo").unwrap_err(),
            @"Invalid first character `.` for identifier `.foo`, expected an underscore or an alphabetic character"
        );
    }

    #[test]
    fn invalid_char() {
        // A dot in module names equals a path separator, which is a separate problem.
        assert_snapshot!(
            Identifier::from_str("foo.bar").unwrap_err(),
            @"Invalid character `.` at position 4 for identifier `foo.bar`, expected an underscore or an alphanumeric character"
        );
        assert_snapshot!(
            Identifier::from_str("foo-bar").unwrap_err(),
            @"Invalid character `-` at position 4 for identifier `foo-bar`, expected an underscore or an alphanumeric character"
        );
        assert_snapshot!(
            Identifier::from_str("foo_bar$").unwrap_err(),
            @"Invalid character `$` at position 8 for identifier `foo_bar$`, expected an underscore or an alphanumeric character"
        );
        assert_snapshot!(
            Identifier::from_str("foo🦀bar").unwrap_err(),
            @"Invalid character `🦀` at position 4 for identifier `foo🦀bar`, expected an underscore or an alphanumeric character"
        );
    }
}
