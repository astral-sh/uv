//! Implementation of PEP 639 cross-language restricted globs.

use glob::{Pattern, PatternError};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Pep639GlobError {
    #[error(transparent)]
    PatternError(#[from] PatternError),
    #[error("The parent directory operator (`..`) at position {pos} is not allowed in license file globs")]
    ParentDirectory { pos: usize },
    #[error("Glob contains invalid character at position {pos}: `{invalid}`")]
    InvalidCharacter { pos: usize, invalid: char },
    #[error("Glob contains invalid character in range at position {pos}: `{invalid}`")]
    InvalidCharacterRange { pos: usize, invalid: char },
}

/// Parse a PEP 639 `license-files` glob.
///
/// The syntax is more restricted than regular globbing in Python or Rust for platform independent
/// results. Since [`glob::Pattern`] is a superset over this format, we can use it after validating
/// that no unsupported features are in the string.
///
/// From [PEP 639](https://peps.python.org/pep-0639/#add-license-files-key):
///
/// > Its value is an array of strings which MUST contain valid glob patterns,
/// > as specified below:
/// >
/// > - Alphanumeric characters, underscores (`_`), hyphens (`-`) and dots (`.`)
/// >   MUST be matched verbatim.
/// >
/// > - Special glob characters: `*`, `?`, `**` and character ranges: `[]`
/// >   containing only the verbatim matched characters MUST be supported.
/// >   Within `[...]`, the hyphen indicates a range (e.g. `a-z`).
/// >   Hyphens at the start or end are matched literally.
/// >
/// > - Path delimiters MUST be the forward slash character (`/`).
/// >   Patterns are relative to the directory containing `pyproject.toml`,
/// >   therefore the leading slash character MUST NOT be used.
/// >
/// > - Parent directory indicators (`..`) MUST NOT be used.
/// >
/// > Any characters or character sequences not covered by this specification are
/// > invalid. Projects MUST NOT use such values.
/// > Tools consuming this field MAY reject invalid values with an error.
pub(crate) fn parse_pep639_glob(glob: &str) -> Result<Pattern, Pep639GlobError> {
    let mut chars = glob.chars().enumerate().peekable();
    // A `..` is on a parent directory indicator at the start of the string or after a directory
    // separator.
    let mut start_or_slash = true;
    while let Some((pos, c)) = chars.next() {
        if c.is_alphanumeric() || matches!(c, '_' | '-' | '*' | '?') {
            start_or_slash = false;
        } else if c == '.' {
            if start_or_slash && matches!(chars.peek(), Some((_, '.'))) {
                return Err(Pep639GlobError::ParentDirectory { pos });
            }
            start_or_slash = false;
        } else if c == '/' {
            start_or_slash = true;
        } else if c == '[' {
            for (pos, c) in chars.by_ref() {
                // TODO: https://discuss.python.org/t/pep-639-round-3-improving-license-clarity-with-better-package-metadata/53020/98
                if c.is_alphanumeric() || matches!(c, '_' | '-' | '.') {
                    // Allowed.
                } else if c == ']' {
                    break;
                } else {
                    return Err(Pep639GlobError::InvalidCharacterRange { pos, invalid: c });
                }
            }
            start_or_slash = false;
        } else {
            return Err(Pep639GlobError::InvalidCharacter { pos, invalid: c });
        }
    }
    Ok(Pattern::new(glob)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use insta::assert_snapshot;

    #[test]
    fn test_error() {
        let parse_err = |glob| parse_pep639_glob(glob).unwrap_err().to_string();
        assert_snapshot!(
            parse_err(".."),
            @"The parent directory operator (`..`) at position 0 is not allowed in license file globs"
        );
        assert_snapshot!(
            parse_err("licenses/.."),
            @"The parent directory operator (`..`) at position 9 is not allowed in license file globs"
        );
        assert_snapshot!(
            parse_err("licenses/LICEN!E.txt"),
            @"Glob contains invalid character at position 14: `!`"
        );
        assert_snapshot!(
            parse_err("licenses/LICEN[!C]E.txt"),
            @"Glob contains invalid character in range at position 15: `!`"
        );
        assert_snapshot!(
            parse_err("licenses/LICEN[C?]E.txt"),
            @"Glob contains invalid character in range at position 16: `?`"
        );
        assert_snapshot!(parse_err("******"), @"Pattern syntax error near position 2: wildcards are either regular `*` or recursive `**`");
        assert_snapshot!(
            parse_err(r"licenses\eula.txt"),
            @r"Glob contains invalid character at position 8: `\`"
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
        ];
        for case in cases {
            parse_pep639_glob(case).unwrap();
        }
    }
}
