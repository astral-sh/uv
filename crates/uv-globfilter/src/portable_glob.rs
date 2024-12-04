//! Cross-language glob syntax from [PEP 639](https://peps.python.org/pep-0639/#add-license-FILES-key).

use globset::{Glob, GlobBuilder};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PortableGlobError {
    /// Shows the failing glob in the error message.
    #[error(transparent)]
    GlobError(#[from] globset::Error),
    #[error(
        "The parent directory operator (`..`) at position {pos} is not allowed in glob: `{glob}`"
    )]
    ParentDirectory { glob: String, pos: usize },
    #[error("Invalid character `{invalid}` at position {pos} in glob: `{glob}`")]
    InvalidCharacter {
        glob: String,
        pos: usize,
        invalid: char,
    },
    #[error("Invalid character `{invalid}` in range at position {pos} in glob: `{glob}`")]
    InvalidCharacterRange {
        glob: String,
        pos: usize,
        invalid: char,
    },
    #[error("Too many at stars at position {pos} in glob: `{glob}`")]
    TooManyStars { glob: String, pos: usize },
}

/// Parse cross-language glob syntax from [PEP 639](https://peps.python.org/pep-0639/#add-license-FILES-key):
///
/// - Alphanumeric characters, underscores (`_`), hyphens (`-`) and dots (`.`) are matched verbatim.
/// - The special glob characters are:
///   - `*`: Matches any number of characters except path separators
///   - `?`: Matches a single character except the path separator
///   - `**`: Matches any number of characters including path separators
///   - `[]`, containing only the verbatim matched characters: Matches a single of the characters contained. Within
///     `[...]`, the hyphen indicates a locale-agnostic range (e.g. `a-z`, order based on Unicode code points). Hyphens at
///     the start or end are matched literally.
/// - The path separator is the forward slash character (`/`). Patterns are relative to the given directory, a leading slash
///   character for absolute paths is not supported.
/// - Parent directory indicators (`..`) are not allowed.
///
/// These rules mean that matching the backslash (`\`) is forbidden, which avoid collisions with the windows path separator.
pub fn parse_portable_glob(glob: &str) -> Result<Glob, PortableGlobError> {
    check_portable_glob(glob)?;
    Ok(GlobBuilder::new(glob).literal_separator(true).build()?)
}

/// See [`parse_portable_glob`].
pub fn check_portable_glob(glob: &str) -> Result<(), PortableGlobError> {
    let mut chars = glob.chars().enumerate().peekable();
    // A `..` is on a parent directory indicator at the start of the string or after a directory
    // separator.
    let mut start_or_slash = true;
    // The number of consecutive stars before the current character.
    while let Some((pos, c)) = chars.next() {
        // `***` or `**literals` can be correctly represented with less stars. They are banned by
        // `glob`, they are allowed by `globset` and PEP 639 is ambiguous, so we're filtering them
        // out.
        if c == '*' {
            let mut star_run = 1;
            while let Some((_, c)) = chars.peek() {
                if *c == '*' {
                    star_run += 1;
                    chars.next();
                } else {
                    break;
                }
            }
            if star_run >= 3 {
                return Err(PortableGlobError::TooManyStars {
                    glob: glob.to_string(),
                    // We don't update pos for the stars.
                    pos,
                });
            } else if star_run == 2 {
                if chars.peek().is_some_and(|(_, c)| *c != '/') {
                    return Err(PortableGlobError::TooManyStars {
                        glob: glob.to_string(),
                        // We don't update pos for the stars.
                        pos,
                    });
                }
            }
            start_or_slash = false;
        } else if c.is_alphanumeric() || matches!(c, '_' | '-' | '?') {
            start_or_slash = false;
        } else if c == '.' {
            if start_or_slash && matches!(chars.peek(), Some((_, '.'))) {
                return Err(PortableGlobError::ParentDirectory {
                    pos,
                    glob: glob.to_string(),
                });
            }
            start_or_slash = false;
        } else if c == '/' {
            start_or_slash = true;
        } else if c == '[' {
            for (pos, c) in chars.by_ref() {
                if c.is_alphanumeric() || matches!(c, '_' | '-' | '.') {
                    // Allowed.
                } else if c == ']' {
                    break;
                } else {
                    return Err(PortableGlobError::InvalidCharacterRange {
                        glob: glob.to_string(),
                        pos,
                        invalid: c,
                    });
                }
            }
            start_or_slash = false;
        } else {
            return Err(PortableGlobError::InvalidCharacter {
                glob: glob.to_string(),
                pos,
                invalid: c,
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use insta::assert_snapshot;

    #[test]
    fn test_error() {
        let parse_err = |glob| parse_portable_glob(glob).unwrap_err().to_string();
        assert_snapshot!(
            parse_err(".."),
            @"The parent directory operator (`..`) at position 0 is not allowed in glob: `..`"
        );
        assert_snapshot!(
            parse_err("licenses/.."),
            @"The parent directory operator (`..`) at position 9 is not allowed in glob: `licenses/..`"
        );
        assert_snapshot!(
            parse_err("licenses/LICEN!E.txt"),
            @"Invalid character `!` at position 14 in glob: `licenses/LICEN!E.txt`"
        );
        assert_snapshot!(
            parse_err("licenses/LICEN[!C]E.txt"),
            @"Invalid character `!` in range at position 15 in glob: `licenses/LICEN[!C]E.txt`"
        );
        assert_snapshot!(
            parse_err("licenses/LICEN[C?]E.txt"),
            @"Invalid character `?` in range at position 16 in glob: `licenses/LICEN[C?]E.txt`"
        );
        assert_snapshot!(
            parse_err("******"),
            @"Too many at stars at position 0 in glob: `******`"
        );
        assert_snapshot!(
            parse_err("licenses/**license"),
            @"Too many at stars at position 9 in glob: `licenses/**license`"
        );
        assert_snapshot!(
            parse_err("licenses/***/licenses.csv"),
            @"Too many at stars at position 9 in glob: `licenses/***/licenses.csv`"
        );
        assert_snapshot!(
            parse_err(r"licenses\eula.txt"),
            @r"Invalid character `\` at position 8 in glob: `licenses\eula.txt`"
        );
    }

    #[test]
    fn test_valid() {
        let cases = [
            "licenses/*.txt",
            "licenses/**/*.txt",
            "LICEN[CS]E.txt",
            "LICEN?E.txt",
            "[a-z].txt",
            "[a-z._-].txt",
            "*/**",
            "LICENSE..txt",
            "LICENSE_file-1.txt",
            // (google translate)
            "licenses/라이센스*.txt",
            "licenses/ライセンス*.txt",
            "licenses/执照*.txt",
            "src/**",
        ];
        for case in cases {
            parse_portable_glob(case).unwrap();
        }
    }
}
