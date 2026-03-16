mod line_wrap;

use std::borrow::Cow;
use std::error::Error;
use std::fmt;
use std::iter;

use owo_colors::{DynColor, OwoColorize};

use line_wrap::{get_wrap_width, wrap_text};

/// An error that may carry user-facing hints.
///
/// Implement this on error types that want to surface contextual suggestions
/// (e.g., "try `--prerelease=allow`") to the diagnostics layer. Hints are
/// rendered after the error output, each prefixed with `hint:`.
pub trait Hint {
    /// Return any hints associated with this error.
    fn hints(&self) -> Hints<'_> {
        Hints::none()
    }
}

/// A collection of user-facing hint messages.
///
/// Each hint is rendered on its own line, prefixed with the styled `hint:` label.
pub struct Hints<'a>(Vec<Cow<'a, str>>);

impl Hints<'_> {
    /// No hints.
    pub fn none() -> Self {
        Self(Vec::new())
    }

    /// Add a single owned hint.
    pub fn push(&mut self, hint: String) {
        self.0.push(Cow::Owned(hint));
    }

    /// Convert all borrowed hints to owned, extending the lifetime to `'static`.
    pub fn into_owned(self) -> Hints<'static> {
        Hints(
            self.0
                .into_iter()
                .map(|cow| Cow::Owned(cow.into_owned()))
                .collect(),
        )
    }

    /// Whether the collection is empty.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Extend with another set of hints, converting borrowed hints to owned.
    pub fn extend(&mut self, other: Hints<'_>) {
        for hint in other.0 {
            let hint = Cow::Owned(hint.into_owned());
            if !self.0.iter().any(|existing| existing == &hint) {
                self.0.push(hint);
            }
        }
    }
}

/// A display adapter for an error followed by its hints.
///
/// Error renderers line-terminate the error before rendering [`Hints`]. Use
/// this adapter when an error and its hints need to be formatted together.
pub struct ErrorWithHints<'a, E> {
    error: E,
    hints: Hints<'a>,
}

impl<'a, E> ErrorWithHints<'a, E> {
    /// Format an error followed by any hints.
    pub fn new(error: E, hints: Hints<'a>) -> Self {
        Self { error, hints }
    }
}

impl<E: fmt::Display> fmt::Display for ErrorWithHints<'_, E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.error)?;
        if !self.hints.is_empty() {
            writeln!(f)?;
            write!(f, "{}", self.hints)?;
        }
        Ok(())
    }
}

impl<'a> From<&'a str> for Hints<'a> {
    fn from(hint: &'a str) -> Self {
        Self(vec![Cow::Borrowed(hint)])
    }
}

impl From<String> for Hints<'_> {
    fn from(hint: String) -> Self {
        Self(vec![Cow::Owned(hint)])
    }
}

impl FromIterator<String> for Hints<'_> {
    fn from_iter<I: IntoIterator<Item = String>>(iter: I) -> Self {
        Self(iter.into_iter().map(Cow::Owned).collect())
    }
}

impl fmt::Display for Hints<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for hint in &self.0 {
            write!(f, "\n{HintPrefix} {hint}")?;
        }
        Ok(())
    }
}

impl<'a> IntoIterator for Hints<'a> {
    type Item = Cow<'a, str>;
    type IntoIter = std::vec::IntoIter<Cow<'a, str>>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

/// A styled `hint:` prefix for use in user-facing messages.
pub struct HintPrefix;

impl fmt::Display for HintPrefix {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", "hint".bold().cyan(), ":".bold())
    }
}

/// Formats an error or warning chain with hints and a custom level and color.
///
/// Each hint is rendered on its own line, prefixed with the styled `hint:` label.
///
/// `width_override` allows callers to override the terminal width for wrapping
/// (primarily for testing). Pass `None` for automatic detection.
pub fn write_error_chain(
    err: &dyn Error,
    mut stream: impl fmt::Write,
    level: impl AsRef<str>,
    color: impl DynColor + Copy,
    hints: Hints<'_>,
    width_override: Option<usize>,
) -> fmt::Result {
    let width = get_wrap_width(width_override);

    let main_msg = err.to_string();
    let main_padding = " ".repeat(level.as_ref().len() + 2);
    let wrapped_main = wrap_text(&main_msg, width, &main_padding, &main_padding);
    writeln!(
        &mut stream,
        "{}{} {}",
        level.as_ref().color(color).bold(),
        ":".bold(),
        wrapped_main.trim()
    )?;

    for source in iter::successors(err.source(), |&err| err.source()) {
        let msg = source.to_string();
        let padding = "  ";
        let cause = "Caused by";
        let child_padding = " ".repeat(padding.len() + cause.len() + 2);

        let wrapped = wrap_text(&msg, width, "", &child_padding);

        let mut lines = wrapped.lines();
        if let Some(first) = lines.next() {
            writeln!(
                &mut stream,
                "{}{}: {}",
                padding,
                cause.color(color).bold(),
                first.trim()
            )?;
            for line in lines {
                if line.trim().is_empty() {
                    writeln!(&mut stream)?;
                } else {
                    writeln!(&mut stream, "{line}")?;
                }
            }
        }
    }

    for hint in hints {
        writeln!(&mut stream, "\n{HintPrefix} {hint}")?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use anyhow::anyhow;
    use indoc::indoc;
    use insta::assert_snapshot;
    use owo_colors::AnsiColors;

    use super::{ErrorWithHints, HintPrefix, Hints, write_error_chain};

    #[test]
    fn extend_deduplicates_matching_hints() {
        let mut hints = Hints::from("same");
        hints.extend(Hints::from("same"));
        hints.extend(Hints::from("other"));

        let hints = hints
            .into_iter()
            .map(std::borrow::Cow::into_owned)
            .collect::<Vec<_>>();
        assert_eq!(hints, vec!["same".to_string(), "other".to_string()]);
    }

    #[test]
    fn error_with_hints_separates_hints_from_error() {
        assert_eq!(
            ErrorWithHints::new("error", Hints::from("fix it")).to_string(),
            format!("error\n\n{HintPrefix} fix it")
        );
        assert_eq!(
            ErrorWithHints::new("error", Hints::none()).to_string(),
            "error"
        );
    }

    #[test]
    fn test_error_wrapping_with_columns() {
        #[derive(Debug, thiserror::Error)]
        #[error(
            "Because fiasobfhuasbf was not found in the package registry and you require fiasobfhuasbf, we can conclude that your requirements are unsatisfiable."
        )]
        struct Inner;

        #[derive(Debug, thiserror::Error)]
        #[error("No solution found when resolving dependencies")]
        struct Outer {
            #[source]
            source: Inner,
        }

        let error = Outer { source: Inner };
        let mut output = String::new();
        write_error_chain(
            &error,
            &mut output,
            "error",
            AnsiColors::Red,
            Hints::none(),
            Some(80),
        )
        .unwrap();
        let output = anstream::adapter::strip_str(&output);

        assert_snapshot!(output, @r"
        error: No solution found when resolving dependencies
          Caused by: Because fiasobfhuasbf was not found in the package registry and you require
                     fiasobfhuasbf, we can conclude that your requirements are
                     unsatisfiable.
        ");
    }

    #[test]
    fn test_error_chain_with_cause() {
        #[derive(Debug, thiserror::Error)]
        #[error("Permission denied")]
        struct Inner;

        #[derive(Debug, thiserror::Error)]
        #[error("Failed to write file")]
        struct Outer {
            #[source]
            source: Inner,
        }

        let error = Outer { source: Inner };
        let mut output = String::new();
        write_error_chain(
            &error,
            &mut output,
            "error",
            AnsiColors::Red,
            Hints::none(),
            None,
        )
        .unwrap();
        let output = anstream::adapter::strip_str(&output);

        assert_snapshot!(output, @r"
        error: Failed to write file
          Caused by: Permission denied
        ");
    }

    #[test]
    fn test_no_hyphenation() {
        #[derive(Debug, thiserror::Error)]
        #[error(
            "Failed to download package from https://files.pythonhosted.org/packages/verylongpackagename"
        )]
        struct LongWord;

        let error = LongWord;
        let mut output = String::new();
        write_error_chain(
            &error,
            &mut output,
            "error",
            AnsiColors::Red,
            Hints::none(),
            Some(50),
        )
        .unwrap();
        let output = anstream::adapter::strip_str(&output);
        assert_snapshot!(output, @r"
        error: Failed to download package from
               https://files.pythonhosted.org/packages/verylongpackagename
        ");
    }

    #[test]
    fn test_long_words_not_broken() {
        #[derive(Debug, thiserror::Error)]
        #[error(
            "The package supercalifragilisticexpialidocious-extraordinarily-long-name was not found"
        )]
        struct VeryLongWord;

        let error = VeryLongWord;
        let mut output = String::new();
        write_error_chain(
            &error,
            &mut output,
            "error",
            AnsiColors::Red,
            Hints::none(),
            Some(40),
        )
        .unwrap();
        let output = anstream::adapter::strip_str(&output);
        assert_snapshot!(output, @r"
        error: The package
               supercalifragilisticexpialidocious-extraordinarily-long-name
               was not found
        ");
    }

    #[test]
    fn test_multiple_error_sources() {
        #[derive(Debug, thiserror::Error)]
        #[error("Network connection timeout after multiple retry attempts")]
        struct DeepError;

        #[derive(Debug, thiserror::Error)]
        #[error("Failed to fetch package metadata from registry")]
        struct MiddleError {
            #[source]
            source: DeepError,
        }

        #[derive(Debug, thiserror::Error)]
        #[error("Unable to resolve package dependencies")]
        struct TopError {
            #[source]
            source: MiddleError,
        }

        let error = TopError {
            source: MiddleError { source: DeepError },
        };
        let mut output = String::new();
        write_error_chain(
            &error,
            &mut output,
            "error",
            AnsiColors::Red,
            Hints::none(),
            Some(60),
        )
        .unwrap();
        let output = anstream::adapter::strip_str(&output);
        assert_snapshot!(output, @r"
        error: Unable to resolve package dependencies
          Caused by: Failed to fetch package metadata from registry
          Caused by: Network connection timeout after multiple retry attempts
        ");
    }

    #[test]
    fn test_multiline_main_message_wraps_each_line() {
        #[derive(Debug, thiserror::Error)]
        #[error(
            "There is no command `foobar` for `uv`. Did you mean one of:\n    auth\n    run\n    init"
        )]
        struct Suggestions;

        let error = Suggestions;
        let mut output = String::new();
        write_error_chain(
            &error,
            &mut output,
            "error",
            AnsiColors::Red,
            Hints::none(),
            Some(50),
        )
        .unwrap();
        let output = anstream::adapter::strip_str(&output);

        assert_snapshot!(output, @r"
        error: There is no command `foobar` for `uv`. Did
               you mean one of:
            auth
            run
            init
        ");
    }

    #[test]
    fn test_wrap_only_on_ascii_space() {
        #[derive(Debug, thiserror::Error)]
        #[error("Path /usr/local/lib/python3.12/site-packages not found in filesystem hierarchy")]
        struct SpecialChars;

        let error = SpecialChars;
        let mut output = String::new();
        write_error_chain(
            &error,
            &mut output,
            "error",
            AnsiColors::Red,
            Hints::none(),
            Some(50),
        )
        .unwrap();
        let output = anstream::adapter::strip_str(&output);
        assert_snapshot!(output, @r"
        error: Path /usr/local/lib/python3.12/site-packages
               not found in filesystem hierarchy
        ");
    }

    #[test]
    fn format_with_hints() {
        let err = anyhow!("Permission denied").context("Failed to fetch package");

        let hints = [
            "Try running with `--verbose` for more information.".to_string(),
            "Try running without --offline.".to_string(),
        ]
        .into_iter()
        .collect();

        let mut rendered = String::new();
        write_error_chain(
            err.as_ref(),
            &mut rendered,
            "error",
            AnsiColors::Red,
            hints,
            None,
        )
        .unwrap();
        let rendered = anstream::adapter::strip_str(&rendered);

        assert_snapshot!(rendered, @r"
        error: Failed to fetch package
          Caused by: Permission denied

        hint: Try running with `--verbose` for more information.

        hint: Try running without --offline.
        ");
    }

    #[test]
    fn format_multiline_message() {
        let err_middle = indoc! {"Failed to fetch https://example.com/upload/python3.13.tar.zst
        Server says: This endpoint only support POST requests.

        For downloads, please refer to https://example.com/download/python3.13.tar.zst"};
        let err = anyhow!("Caused By: HTTP Error 400")
            .context(err_middle)
            .context("Failed to download Python 3.12");

        let mut rendered = String::new();
        write_error_chain(
            err.as_ref(),
            &mut rendered,
            "error",
            AnsiColors::Red,
            Hints::none(),
            None,
        )
        .unwrap();
        let rendered = anstream::adapter::strip_str(&rendered);

        assert_snapshot!(rendered, @r"
        error: Failed to download Python 3.12
          Caused by: Failed to fetch https://example.com/upload/python3.13.tar.zst
        Server says: This endpoint only support POST requests.

        For downloads, please refer to https://example.com/download/python3.13.tar.zst
          Caused by: Caused By: HTTP Error 400
        ");
    }
}
