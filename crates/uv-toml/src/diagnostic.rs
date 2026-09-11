use std::error::Error;
use std::fmt;
use std::ops::Range;

use uv_errors::{Diagnostic, SourceFile, SourceSnippet};

/// A transparently displayed parser error with optional retained input.
///
/// The original error is not rewritten, and its source chain is unchanged. Callers that already
/// expose the original error as a source can instead retain a [`SourceFile`] beside that source.
#[derive(Debug)]
pub struct ParseError<E> {
    error: E,
    document: Option<SourceFile>,
}

impl<E> ParseError<E> {
    /// Retain the exact decoded document passed to the parser.
    pub fn new(error: E, document: SourceFile) -> Self {
        Self {
            error,
            document: Some(document),
        }
    }

    /// The original parser error.
    pub fn original(&self) -> &E {
        &self.error
    }

    /// The source snapshot retained by the parse boundary, if available.
    pub fn document(&self) -> Option<&SourceFile> {
        self.document.as_ref()
    }
}

impl<E> From<E> for ParseError<E> {
    fn from(error: E) -> Self {
        Self {
            error,
            document: None,
        }
    }
}

impl<E: fmt::Display> fmt::Display for ParseError<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.error.fmt(formatter)
    }
}

impl<E: Error + 'static> Error for ParseError<E> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.error.source()
    }
}

/// Describe a TOML parser message using its typed span and retained input.
///
/// An unavailable or invalid span leaves the original parser display in use, including any
/// deserialization key path. Only the first physical line touched by the span is displayed.
pub fn diagnostic_for_span<'a>(
    message: &'a str,
    span: Option<Range<usize>>,
    document: &SourceFile,
) -> Option<Diagnostic<'a>> {
    let snippet = SourceSnippet::new(document.clone(), span?)?;
    Some(Diagnostic::new(message).with_snippet(snippet))
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use insta::assert_snapshot;
    use serde::Deserialize;
    use uv_errors::SourceFile;

    use super::{ParseError, diagnostic_for_span};

    #[test]
    fn retains_the_original_parser_error() {
        #[derive(Deserialize)]
        struct Settings {
            #[serde(rename = "count")]
            _count: usize,
        }

        let input = "count = 'invalid'";
        let original = toml::from_str::<Settings>(input)
            .err()
            .expect("invalid integer in test input");
        let error = ParseError::new(original, SourceFile::new("uv.toml", input));
        assert!(error.source().is_none());
        assert!(
            diagnostic_for_span(
                error.original().message(),
                error.original().span(),
                error.document().expect("retained source"),
            )
            .is_some()
        );
        assert_snapshot!(error, @r#"
        TOML parse error at line 1, column 9
          |
        1 | count = 'invalid'
          |         ^^^^^^^^^
        invalid type: string "invalid", expected usize
        "#);

        let original = <toml::de::Error as serde::de::Error>::custom("invalid settings");
        let error = ParseError::from(original);
        assert!(error.document().is_none());
        assert!(
            diagnostic_for_span(
                error.original().message(),
                error.original().span(),
                &SourceFile::new("uv.toml", input),
            )
            .is_none()
        );
    }

    #[test]
    fn spanless_deserialization_retains_the_key_path() {
        #[derive(Deserialize)]
        struct Settings {
            #[serde(rename = "outer")]
            _outer: Nested,
        }

        #[derive(Deserialize)]
        struct Nested {
            #[serde(rename = "count")]
            _count: usize,
        }

        let input = "[outer]\ncount = 'invalid'";
        let value = toml::from_str::<toml::Value>(input).expect("valid TOML in test input");
        let original = value
            .try_into::<Settings>()
            .err()
            .expect("invalid integer in test input");
        assert!(original.span().is_none());
        let error = ParseError::new(original, SourceFile::new("uv.toml", input));
        assert!(
            diagnostic_for_span(
                error.original().message(),
                error.original().span(),
                error.document().expect("retained source"),
            )
            .is_none()
        );
        assert_snapshot!(error, @r#"
        invalid type: string "invalid", expected usize
        in `outer.count`
        "#);
    }
}
