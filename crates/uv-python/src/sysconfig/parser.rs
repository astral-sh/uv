use std::collections::BTreeMap;
use std::str::FromStr;

use serde::Serialize;
use serde_json::ser::PrettyFormatter;

use crate::sysconfig::cursor::Cursor;

/// A value in the [`SysconfigData`] map.
///
/// Values are assumed to be either strings or integers.
#[derive(Debug, Clone, Eq, PartialEq, serde::Serialize)]
#[serde(untagged)]
pub(super) enum Value {
    String(String),
    Int(i32),
}

/// The data extracted from a `_sysconfigdata_` file.
#[derive(Debug, Clone, Eq, PartialEq, serde::Serialize)]
pub(super) struct SysconfigData(BTreeMap<String, Value>);

impl SysconfigData {
    /// Returns an iterator over the key-value pairs in the map.
    pub(super) fn iter_mut(&mut self) -> std::collections::btree_map::IterMut<String, Value> {
        self.0.iter_mut()
    }

    /// Inserts a key-value pair into the map.
    pub(super) fn insert(&mut self, key: String, value: Value) -> Option<Value> {
        self.0.insert(key, value)
    }

    /// Formats the `sysconfig` data as a pretty-printed string.
    pub(super) fn to_string_pretty(&self) -> Result<String, serde_json::Error> {
        let output = {
            let mut buf = Vec::new();
            let mut serializer = serde_json::Serializer::with_formatter(
                &mut buf,
                PrettyFormatter::with_indent(b"    "),
            );
            self.0.serialize(&mut serializer)?;
            String::from_utf8(buf).unwrap()
        };
        Ok(format!(
            "# system configuration generated and used by the sysconfig module\nbuild_time_vars = {output}\n",
        ))
    }
}

impl std::fmt::Display for SysconfigData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let output = {
            let mut buf = Vec::new();
            let mut serializer = serde_json::Serializer::new(&mut buf);
            self.0.serialize(&mut serializer).unwrap();
            String::from_utf8(buf).unwrap()
        };
        write!(f, "{output}")
    }
}

impl FromIterator<(String, Value)> for SysconfigData {
    fn from_iter<T: IntoIterator<Item = (String, Value)>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

/// Parse the `_sysconfigdata_` file (e.g., `{real_prefix}/lib/python3.12/_sysconfigdata__darwin_darwin.py"`
/// on macOS).
///
/// `_sysconfigdata_` is structured as follows:
///
/// 1. A comment on the first line (e.g., `# system configuration generated and used by the sysconfig module`).
/// 2. An assignment to `build_time_vars` (e.g., `build_time_vars = { ... }`).
///
/// The right-hand side of the assignment is a JSON object. The keys are strings, and the values
/// are strings or numbers.
impl FromStr for SysconfigData {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Read the first line of the file.
        let Some(s) =
            s.strip_prefix("# system configuration generated and used by the sysconfig module\n")
        else {
            return Err(Error::MissingHeader);
        };

        // Read the assignment to `build_time_vars`.
        let Some(s) = s.strip_prefix("build_time_vars") else {
            return Err(Error::MissingAssignment);
        };

        let mut cursor = Cursor::new(s);

        cursor.eat_while(is_python_whitespace);
        if !cursor.eat_char('=') {
            return Err(Error::MissingAssignment);
        }
        cursor.eat_while(is_python_whitespace);

        if !cursor.eat_char('{') {
            return Err(Error::MissingOpenBrace);
        }

        let mut map = BTreeMap::new();
        loop {
            let Some(next) = cursor.bump() else {
                return Err(Error::UnexpectedEof);
            };

            match next {
                '\'' | '"' => {
                    // Parse key.
                    let key = parse_string(&mut cursor, next)?;

                    cursor.eat_while(is_python_whitespace);
                    cursor.eat_char(':');
                    cursor.eat_while(is_python_whitespace);

                    // Parse value
                    let value = match cursor.first() {
                        '\'' | '"' => Value::String(parse_concatenated_string(&mut cursor)?),
                        '-' => {
                            cursor.bump();
                            Value::Int(-parse_int(&mut cursor)?)
                        }
                        c if c.is_ascii_digit() => Value::Int(parse_int(&mut cursor)?),
                        c => return Err(Error::UnexpectedCharacter(c)),
                    };

                    // Insert into map.
                    map.insert(key, value);

                    // Skip optional comma.
                    cursor.eat_while(is_python_whitespace);
                    cursor.eat_char(',');
                    cursor.eat_while(is_python_whitespace);
                }

                // Skip whitespace.
                ' ' | '\n' | '\r' | '\t' => {}

                // When we see a closing brace, we're done.
                '}' => {
                    break;
                }

                c => return Err(Error::UnexpectedCharacter(c)),
            }
        }

        Ok(Self(map))
    }
}

/// Parse a Python string literal.
///
/// Expects the previous character to be the opening quote character.
fn parse_string(cursor: &mut Cursor, quote: char) -> Result<String, Error> {
    let mut result = String::new();
    loop {
        let Some(c) = cursor.bump() else {
            return Err(Error::UnexpectedEof);
        };
        match c {
            '\\' => {
                // Treat the next character as a literal.
                //
                // See: https://github.com/astral-sh/ruff/blob/d47fba1e4aeeb18085900dfbbcd187e90d536913/crates/ruff_python_parser/src/string.rs#L194
                let Some(c) = cursor.bump() else {
                    return Err(Error::UnexpectedEof);
                };
                result.push(match c {
                    '\\' => '\\',
                    '\'' => '\'',
                    '\"' => '"',
                    _ => {
                        return Err(Error::UnrecognizedEscape(c));
                    }
                });
            }

            // Consume closing quote.
            c if c == quote => {
                break;
            }

            c => {
                result.push(c);
            }
        }
    }
    Ok(result)
}

/// Parse a Python string, which may be a concatenation of multiple string literals.
///
/// Expects the cursor to start at an opening quote character.
fn parse_concatenated_string(cursor: &mut Cursor) -> Result<String, Error> {
    let mut result = String::new();
    loop {
        let Some(c) = cursor.bump() else {
            return Err(Error::UnexpectedEof);
        };
        match c {
            '\'' | '"' => {
                // Parse a new string fragment and append it.
                result.push_str(&parse_string(cursor, c)?);
            }
            c if is_python_whitespace(c) => {
                // Skip whitespace between fragments
            }
            c => return Err(Error::UnexpectedCharacter(c)),
        }

        // Lookahead to the end of the string.
        if matches!(cursor.first(), ',' | '}') {
            break;
        }
    }
    Ok(result)
}

/// Parse an integer literal.
///
/// Expects the cursor to start at the first digit of the integer.
fn parse_int(cursor: &mut Cursor) -> Result<i32, std::num::ParseIntError> {
    let mut result = String::new();
    loop {
        let c = cursor.first();
        if !c.is_ascii_digit() {
            break;
        }
        result.push(c);
        cursor.bump();
    }
    result.parse()
}

/// Returns `true` for [whitespace](https://docs.python.org/3/reference/lexical_analysis.html#whitespace-between-tokens)
/// characters.
const fn is_python_whitespace(c: char) -> bool {
    matches!(
        c,
        // Space, tab, form-feed, newline, or carriage return
        ' ' | '\t' | '\x0C' | '\n' | '\r'
    )
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Missing opening brace")]
    MissingOpenBrace,
    #[error("Unexpected character: {0}")]
    UnexpectedCharacter(char),
    #[error("Unexpected end of file")]
    UnexpectedEof,
    #[error("Unrecognized escape sequence: {0}")]
    UnrecognizedEscape(char),
    #[error("Failed to parse integer")]
    ParseInt(#[from] std::num::ParseIntError),
    #[error("`_sysconfigdata_` is missing a header comment")]
    MissingHeader,
    #[error("`_sysconfigdata_` is missing an assignment to `build_time_vars`")]
    MissingAssignment,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_string() {
        let input = indoc::indoc!(
            r#"
            # system configuration generated and used by the sysconfig module
            build_time_vars = {
                "key1": "value1",
                "key2": 42,
                "key3": "multi-part" " string"
            }
        "#
        );

        let result = input.parse::<SysconfigData>().expect("Parsing failed");
        let snapshot = result.to_string_pretty().unwrap();

        insta::assert_snapshot!(snapshot, @r###"
        # system configuration generated and used by the sysconfig module
        build_time_vars = {
            "key1": "value1",
            "key2": 42,
            "key3": "multi-part string"
        }
        "###);
    }

    #[test]
    fn test_parse_backslash() {
        let input = indoc::indoc!(
            r#"
            # system configuration generated and used by the sysconfig module
            build_time_vars = {
                "key1": "value1\"value2\"value3",
                "key2": "value1\\value2\\value3",
                "key3": "value1\\\"value2\\\"value3",
                "key4": "value1\\\\value2\\\\value3",
            }
        "#
        );

        let result = input.parse::<SysconfigData>().expect("Parsing failed");
        let snapshot = result.to_string_pretty().unwrap();

        insta::assert_snapshot!(snapshot, @r###"
        # system configuration generated and used by the sysconfig module
        build_time_vars = {
            "key1": "value1\"value2\"value3",
            "key2": "value1\\value2\\value3",
            "key3": "value1\\\"value2\\\"value3",
            "key4": "value1\\\\value2\\\\value3"
        }
        "###);
    }

    #[test]
    fn test_parse_trailing_backslash() {
        let input = indoc::indoc!(
            r#"
            # system configuration generated and used by the sysconfig module
            build_time_vars = {
                "key1": "value1\\value2\\value3\",
            }
        "#
        );

        let result = input.parse::<SysconfigData>();
        assert!(matches!(result, Err(Error::UnexpectedEof)));
    }

    #[test]
    fn test_parse_unrecognized_escape() {
        let input = indoc::indoc!(
            r#"
            # system configuration generated and used by the sysconfig module
            build_time_vars = {
                "key1": "value1\value2",
            }
        "#
        );

        let result = input.parse::<SysconfigData>();
        assert!(matches!(result, Err(Error::UnrecognizedEscape('v'))));
    }

    #[test]
    fn test_parse_trailing_comma() {
        let input = indoc::indoc!(
            r#"
            # system configuration generated and used by the sysconfig module
            build_time_vars = {
                "key1": "value1",
                "key2": 42,
                "key3": "multi-part" " string",
            }
        "#
        );

        let result = input.parse::<SysconfigData>().expect("Parsing failed");
        let snapshot = result.to_string_pretty().unwrap();

        insta::assert_snapshot!(snapshot, @r###"
        # system configuration generated and used by the sysconfig module
        build_time_vars = {
            "key1": "value1",
            "key2": 42,
            "key3": "multi-part string"
        }
        "###);
    }

    #[test]
    fn test_parse_integer_values() {
        let input = indoc::indoc!(
            r#"
            # system configuration generated and used by the sysconfig module
            build_time_vars = {
                "key1": 12345,
                "key2": -15
            }
        "#
        );

        let result = input.parse::<SysconfigData>().expect("Parsing failed");
        let snapshot = result.to_string_pretty().unwrap();

        insta::assert_snapshot!(snapshot, @r###"
        # system configuration generated and used by the sysconfig module
        build_time_vars = {
            "key1": 12345,
            "key2": -15
        }
        "###);
    }

    #[test]
    fn test_parse_escaped_quotes() {
        let input = indoc::indoc!(
            r#"
            # system configuration generated and used by the sysconfig module
            build_time_vars = {
                "key1": "value with \"escaped quotes\"",
                "key2": 'single-quoted \'escaped\''
            }
        "#
        );

        let result = input.parse::<SysconfigData>().expect("Parsing failed");
        let snapshot = result.to_string_pretty().unwrap();

        insta::assert_snapshot!(snapshot, @r###"
        # system configuration generated and used by the sysconfig module
        build_time_vars = {
            "key1": "value with \"escaped quotes\"",
            "key2": "single-quoted 'escaped'"
        }
        "###);
    }

    #[test]
    fn test_parse_concatenated_strings() {
        let input = indoc::indoc!(
            r#"
            # system configuration generated and used by the sysconfig module
            build_time_vars = {
                "key1": "multi-"
                        "line "
                        "string"
            }
        "#
        );

        let result = input.parse::<SysconfigData>().expect("Parsing failed");
        let snapshot = result.to_string_pretty().unwrap();

        insta::assert_snapshot!(snapshot, @r###"
        # system configuration generated and used by the sysconfig module
        build_time_vars = {
            "key1": "multi-line string"
        }
        "###);
    }

    #[test]
    fn test_missing_header_error() {
        let input = indoc::indoc!(
            r#"
            build_time_vars = {
                "key1": "value1"
            }
        "#
        );

        let result = input.parse::<SysconfigData>();
        assert!(matches!(result, Err(Error::MissingHeader)));
    }

    #[test]
    fn test_missing_assignment_error() {
        let input = indoc::indoc!(
            r#"
            # system configuration generated and used by the sysconfig module
            {
                "key1": "value1"
            }
        "#
        );

        let result = input.parse::<SysconfigData>();
        assert!(matches!(result, Err(Error::MissingAssignment)));
    }

    #[test]
    fn test_unexpected_character_error() {
        let input = indoc::indoc!(
            r#"
            # system configuration generated and used by the sysconfig module
            build_time_vars = {
                "key1": &123
            }
        "#
        );

        let result = input.parse::<SysconfigData>();
        assert!(
            result.is_err(),
            "Expected parsing to fail due to unexpected character"
        );
    }

    #[test]
    fn test_unexpected_eof() {
        let input = indoc::indoc!(
            r#"
            # system configuration generated and used by the sysconfig module
            build_time_vars = {
                "key1": 123
        "#
        );

        let result = input.parse::<SysconfigData>();
        assert!(
            result.is_err(),
            "Expected parsing to fail due to unexpected character"
        );
    }

    #[test]
    fn test_unexpected_comma() {
        let input = indoc::indoc!(
            r#"
            # system configuration generated and used by the sysconfig module
            build_time_vars = {
                "key1": 123,,
            }
        "#
        );

        let result = input.parse::<SysconfigData>();
        assert!(
            result.is_err(),
            "Expected parsing to fail due to unexpected character"
        );
    }
}
