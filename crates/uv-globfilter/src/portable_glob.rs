//! Cross-language glob syntax from
//! [PEP 639](https://packaging.python.org/en/latest/specifications/glob-patterns/).

use globset::{Glob, GlobBuilder};
use owo_colors::OwoColorize;
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
    #[error(
        "Invalid character `{invalid}` at position {pos} in glob: `{glob}`. {}{} Characters can be escaped with a backslash",
        "hint".bold().cyan(),
        ":".bold()
    )]
    InvalidCharacterUv {
        glob: String,
        pos: usize,
        invalid: char,
    },
    #[error(
        "Only forward slashes are allowed as path separator, invalid character at position {pos} in glob: `{glob}`"
    )]
    InvalidBackslash { glob: String, pos: usize },
    #[error(
        "Path separators can't be escaped, invalid character at position {pos} in glob: `{glob}`"
    )]
    InvalidEscapee { glob: String, pos: usize },
    #[error("Invalid character `{invalid}` in range at position {pos} in glob: `{glob}`")]
    InvalidCharacterRange {
        glob: String,
        pos: usize,
        invalid: char,
    },
    #[error("Too many at stars at position {pos} in glob: `{glob}`")]
    TooManyStars { glob: String, pos: usize },
    #[error("Trailing backslash at position {pos} in glob: `{glob}`")]
    TrailingEscape { glob: String, pos: usize },
}

/// Cross-language glob syntax from
/// [PEP 639](https://packaging.python.org/en/latest/specifications/glob-patterns/).
///
/// The variant determines whether the parser strictly adheres to PEP 639 rules or allows extensions
/// such as backslash escapes.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum PortableGlobParser {
    /// Follow the PEP 639 rules strictly.
    Pep639,
    /// In addition to the PEP 639 syntax, allow escaping characters with backslashes.
    ///
    /// For cross-platform compatibility, escaping path separators is not allowed, i.e., forward
    /// slashes and backslashes can't be escaped.
    Uv,
}

impl PortableGlobParser {
    fn backslash_escape(self) -> bool {
        match self {
            Self::Pep639 => false,
            Self::Uv => true,
        }
    }

    /// Parse cross-language glob syntax based on [PEP 639](https://packaging.python.org/en/latest/specifications/glob-patterns/):
    ///
    /// - Alphanumeric characters, underscores (`_`), hyphens (`-`) and dots (`.`) are matched verbatim.
    /// - The special glob characters are:
    ///   - `*`: Matches any number of characters except path separators
    ///   - `?`: Matches a single character except the path separator
    ///   - `**`: Matches any number of characters including path separators
    ///   - `[]`, containing only the verbatim matched characters: Matches a single of the characters contained. Within
    ///     `[...]`, the hyphen indicates a locale-agnostic range (e.g. `a-z`, order based on Unicode code points). Hyphens at
    ///     the start or end are matched literally.
    ///   - `\`: Disallowed in PEP 639 mode. In uv mode, it escapes the following character to be matched verbatim.
    /// - The path separator is the forward slash character (`/`). Patterns are relative to the given directory, a leading slash
    ///   character for absolute paths is not supported.
    /// - Parent directory indicators (`..`) are not allowed.
    ///
    /// These rules mean that matching the backslash (`\`) is forbidden, which avoid collisions with the windows path separator.
    pub fn parse(&self, glob: &str) -> Result<Glob, PortableGlobError> {
        self.check(glob)?;
        Ok(GlobBuilder::new(glob)
            .literal_separator(true)
            // No need to support Windows-style paths, so the backslash can be used a escape.
            .backslash_escape(self.backslash_escape())
            .build()?)
    }

    /// See [`parse_portable_glob`].
    pub fn check(&self, glob: &str) -> Result<(), PortableGlobError> {
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
            } else if c == '\\' {
                match self {
                    Self::Pep639 => {
                        return Err(PortableGlobError::InvalidBackslash {
                            glob: glob.to_string(),
                            pos,
                        });
                    }
                    Self::Uv => {
                        match chars.next() {
                            Some((pos, '/' | '\\')) => {
                                // For cross-platform compatibility, we don't allow forward slashes or
                                // backslashes to be escaped.
                                return Err(PortableGlobError::InvalidEscapee {
                                    glob: glob.to_string(),
                                    pos,
                                });
                            }
                            Some(_) => {
                                // Escaped character
                            }
                            None => {
                                return Err(PortableGlobError::TrailingEscape {
                                    glob: glob.to_string(),
                                    pos,
                                });
                            }
                        }
                    }
                }
            } else {
                let err = match self {
                    Self::Pep639 => PortableGlobError::InvalidCharacter {
                        glob: glob.to_string(),
                        pos,
                        invalid: c,
                    },
                    Self::Uv => PortableGlobError::InvalidCharacterUv {
                        glob: glob.to_string(),
                        pos,
                        invalid: c,
                    },
                };
                return Err(err);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use insta::assert_snapshot;

    #[test]
    fn test_error() {
        let parse_err = |glob| {
            let error = PortableGlobParser::Pep639.parse(glob).unwrap_err();
            anstream::adapter::strip_str(&error.to_string()).to_string()
        };
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
            @r"Only forward slashes are allowed as path separator, invalid character at position 8 in glob: `licenses\eula.txt`"
        );
        assert_snapshot!(
            parse_err(r"**/@test"),
            @"Invalid character `@` at position 3 in glob: `**/@test`"
        );
        // Escapes are not allowed in strict PEP 639 mode
        assert_snapshot!(
            parse_err(r"public domain/Gulliver\\’s Travels.txt"),
            @r"Invalid character ` ` at position 6 in glob: `public domain/Gulliver\\’s Travels.txt`"
        );
        let parse_err_uv = |glob| {
            let error = PortableGlobParser::Uv.parse(glob).unwrap_err();
            anstream::adapter::strip_str(&error.to_string()).to_string()
        };
        assert_snapshot!(
            parse_err_uv(r"**/@test"),
            @"Invalid character `@` at position 3 in glob: `**/@test`. hint: Characters can be escaped with a backslash"
        );
        // Escaping slashes is not allowed.
        assert_snapshot!(
            parse_err_uv(r"licenses\\MIT.txt"),
            @r"Path separators can't be escaped, invalid character at position 9 in glob: `licenses\\MIT.txt`"
        );
        assert_snapshot!(
            parse_err_uv(r"licenses\/MIT.txt"),
            @r"Path separators can't be escaped, invalid character at position 9 in glob: `licenses\/MIT.txt`"
        );
    }

    #[test]
    fn test_valid() {
        let cases = [
            r"licenses/*.txt",
            r"licenses/**/*.txt",
            r"LICEN[CS]E.txt",
            r"LICEN?E.txt",
            r"[a-z].txt",
            r"[a-z._-].txt",
            r"*/**",
            r"LICENSE..txt",
            r"LICENSE_file-1.txt",
            // (google translate)
            r"licenses/라이센스*.txt",
            r"licenses/ライセンス*.txt",
            r"licenses/执照*.txt",
            r"src/**",
        ];
        let cases_uv = [
            r"public-domain/Gulliver\’s\ Travels.txt",
            // https://github.com/astral-sh/uv/issues/13280
            r"**/\@test",
        ];
        for case in cases {
            PortableGlobParser::Pep639.parse(case).unwrap();
        }
        for case in cases.iter().chain(cases_uv.iter()) {
            PortableGlobParser::Uv.parse(case).unwrap();
        }
    }
}
