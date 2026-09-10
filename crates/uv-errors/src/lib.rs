mod line_wrap;

use std::borrow::Cow;
use std::error::Error;
use std::fmt;
use std::iter;

use owo_colors::{AnsiColors, DynColor, OwoColorize};

use line_wrap::{get_wrap_width, wrap_text};

/// An error that may carry user-facing hints.
///
/// Implement this on error types that want to surface contextual suggestions
/// (e.g., "try `--prerelease=allow`") to the diagnostics layer. Hints are
/// rendered after the error output, each prefixed with `hint:`.
pub trait Hinted {
    /// Return any hints associated with this error.
    fn hints(&self) -> Hints<'_> {
        Hints::none()
    }
}

/// The display order of a user-facing hint.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum HintOrdering {
    /// Advice that should be shown before other hints.
    First,
    /// Advice with no preferred placement.
    #[default]
    Any,
    /// General advice that should follow more specific hints.
    Last,
}

/// A user-facing hint and its preferred display order.
pub struct HintMessage<'a> {
    message: Cow<'a, str>,
    ordering: HintOrdering,
}

impl<'a> HintMessage<'a> {
    /// Create a hint with no preferred placement.
    pub fn new(message: impl Into<Cow<'a, str>>) -> Self {
        Self {
            message: message.into(),
            ordering: HintOrdering::default(),
        }
    }

    /// Set the preferred display order of this hint.
    #[must_use]
    pub fn with_ordering(mut self, ordering: HintOrdering) -> Self {
        self.ordering = ordering;
        self
    }

    /// Convert a borrowed hint to owned, extending its lifetime to `'static`.
    fn into_owned(self) -> HintMessage<'static> {
        HintMessage {
            message: Cow::Owned(self.message.into_owned()),
            ordering: self.ordering,
        }
    }
}

impl<'a> From<&'a str> for HintMessage<'a> {
    fn from(message: &'a str) -> Self {
        Self::new(message)
    }
}

impl From<String> for HintMessage<'_> {
    fn from(message: String) -> Self {
        Self::new(message)
    }
}

/// A collection of user-facing hint messages.
///
/// Each hint is rendered on its own line, prefixed with the styled `hint:` label.
/// Hints are grouped by [`HintOrdering`], retaining insertion order within each group.
pub struct Hints<'a>(Vec<HintMessage<'a>>);

impl<'a> Hints<'a> {
    /// No hints.
    pub fn none() -> Self {
        Self(Vec::new())
    }

    /// Add a single hint.
    pub fn push(&mut self, hint: impl Into<HintMessage<'a>>) {
        self.0.push(hint.into());
    }

    /// Set the display order of every hint in this collection.
    #[must_use]
    pub fn with_ordering(mut self, ordering: HintOrdering) -> Self {
        for hint in &mut self.0 {
            hint.ordering = ordering;
        }
        self
    }

    /// Convert all borrowed hints to owned, extending the lifetime to `'static`.
    pub fn into_owned(self) -> Hints<'static> {
        Hints(self.0.into_iter().map(HintMessage::into_owned).collect())
    }

    /// Whether the collection is empty.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Iterate over hint messages in display order.
    pub fn iter(&self) -> impl Iterator<Item = &str> {
        [HintOrdering::First, HintOrdering::Any, HintOrdering::Last]
            .into_iter()
            .flat_map(|ordering| {
                self.0
                    .iter()
                    .filter(move |hint| hint.ordering == ordering)
                    .map(|hint| hint.message.as_ref())
            })
    }

    /// Extend with another set of hints, converting borrowed hints to owned.
    ///
    /// Duplicate messages retain their first insertion position and earliest ordering.
    pub fn extend(&mut self, other: Hints<'_>) {
        for hint in other.0 {
            if let Some(existing) = self
                .0
                .iter_mut()
                .find(|existing| existing.message == hint.message)
            {
                existing.ordering = existing.ordering.min(hint.ordering);
            } else {
                self.0.push(hint.into_owned());
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
        Self::from(HintMessage::from(hint))
    }
}

impl From<String> for Hints<'_> {
    fn from(hint: String) -> Self {
        Self::from(HintMessage::from(hint))
    }
}

impl<'a> From<HintMessage<'a>> for Hints<'a> {
    fn from(hint: HintMessage<'a>) -> Self {
        Self(vec![hint])
    }
}

impl<'a, T: Into<HintMessage<'a>>> FromIterator<T> for Hints<'a> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        Self(iter.into_iter().map(Into::into).collect())
    }
}

impl fmt::Display for Hints<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for hint in self.iter() {
            write!(f, "\n{HintPrefix} {hint}")?;
        }
        Ok(())
    }
}

/// A styled `hint:` prefix for use in user-facing messages.
pub struct HintPrefix;

impl fmt::Display for HintPrefix {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", "hint".bold().cyan(), ":".bold())
    }
}

/// Options for formatting an error chain.
#[must_use]
pub struct ErrorOptions<'a, C = AnsiColors, W = Stderr> {
    level: Cow<'a, str>,
    color: C,
    width_override: Option<usize>,
    stream: W,
}

/// A standard-error writer for formatted error chains.
#[derive(Debug, Clone, Copy, Default)]
pub struct Stderr;

impl fmt::Write for Stderr {
    fn write_str(&mut self, output: &str) -> fmt::Result {
        anstream::eprint!("{output}");
        Ok(())
    }
}

impl Default for ErrorOptions<'_, AnsiColors, Stderr> {
    fn default() -> Self {
        Self {
            level: Cow::Borrowed("error"),
            color: AnsiColors::Red,
            width_override: None,
            stream: Stderr,
        }
    }
}

impl<'a, C, W> ErrorOptions<'a, C, W> {
    /// Use a custom level prefix, such as `warning`.
    pub fn with_level(mut self, level: impl Into<Cow<'a, str>>) -> Self {
        self.level = level.into();
        self
    }

    /// Use a custom color for the level and cause prefixes.
    pub fn with_color<D>(self, color: D) -> ErrorOptions<'a, D, W> {
        ErrorOptions {
            level: self.level,
            color,
            width_override: self.width_override,
            stream: self.stream,
        }
    }

    /// Override the terminal width used for wrapping.
    ///
    /// This is primarily useful for testing.
    #[cfg(test)]
    fn with_width_override(mut self, width_override: usize) -> Self {
        self.width_override = Some(width_override);
        self
    }

    /// Write the rendered error chain to a custom stream.
    pub fn with_stream<D>(self, stream: D) -> ErrorOptions<'a, C, D> {
        ErrorOptions {
            level: self.level,
            color: self.color,
            width_override: self.width_override,
            stream,
        }
    }
}

/// Format an error chain and explicitly supplied hints to standard error using the default level
/// and color.
pub fn write_error_chain(err: &dyn Error, hints: &Hints<'_>) -> fmt::Result {
    write_error_chain_with_options(err, hints, ErrorOptions::default())
}

/// Format the [`Debug`] representation of every error in an error chain.
pub fn debug_error_chain(err: &dyn Error) -> impl fmt::Display + '_ {
    DebugErrorChain(err)
}

struct DebugErrorChain<'a>(&'a dyn Error);

impl fmt::Display for DebugErrorChain<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, error) in iter::successors(Some(self.0), |&error| error.source()).enumerate() {
            if index > 0 {
                formatter.write_str("\n")?;
            }
            write!(formatter, "{index}: {error:?}")?;
        }
        Ok(())
    }
}

/// Formats an error or warning chain with custom options.
///
/// Each hint is rendered on its own line, prefixed with the styled `hint:` label.
pub fn write_error_chain_with_options<C: DynColor + Copy, W: fmt::Write>(
    err: &dyn Error,
    hints: &Hints<'_>,
    options: ErrorOptions<'_, C, W>,
) -> fmt::Result {
    let ErrorOptions {
        level,
        color,
        width_override,
        mut stream,
    } = options;
    let width = get_wrap_width(width_override);

    let main_msg = err.to_string();
    let main_padding = " ".repeat(level.len() + 2);
    let wrapped_main = wrap_text(&main_msg, width, &main_padding, &main_padding, "");
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
        let authored_line_padding = "    ";

        let wrapped = wrap_text(&msg, width, "", &child_padding, authored_line_padding);

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

    for hint in hints.iter() {
        writeln!(&mut stream, "\n{HintPrefix} {hint}")?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use anyhow::anyhow;
    use indoc::indoc;
    use insta::{assert_debug_snapshot, assert_snapshot};
    use owo_colors::AnsiColors;

    use super::{
        ErrorOptions, ErrorWithHints, HintMessage, HintOrdering, Hints, debug_error_chain,
        write_error_chain_with_options,
    };

    #[test]
    fn extend_deduplicates_matching_hints() {
        let mut hints = Hints::from("same");
        hints.extend(Hints::from("same"));
        hints.extend(Hints::from("other"));

        let hints = hints.iter().collect::<Vec<_>>();
        assert_debug_snapshot!(hints, @r#"
        [
            "same",
            "other",
        ]
        "#);
    }

    #[test]
    fn hint_ordering_retains_insertion_order() {
        let mut hints = Hints::from("any 1");
        hints.push(HintMessage::new("last 1").with_ordering(HintOrdering::Last));
        hints.push(HintMessage::new("first 1").with_ordering(HintOrdering::First));
        hints.push("any 2".to_string());
        hints.extend(Hints::from("last 2").with_ordering(HintOrdering::Last));
        hints.extend(Hints::from("first 2").with_ordering(HintOrdering::First));

        assert_snapshot!(anstream::adapter::strip_str(&hints.to_string()), @"
        hint: first 1
        hint: first 2
        hint: any 1
        hint: any 2
        hint: last 1
        hint: last 2
        ");
        assert_debug_snapshot!(hints.iter().collect::<Vec<_>>(), @r#"
        [
            "first 1",
            "first 2",
            "any 1",
            "any 2",
            "last 1",
            "last 2",
        ]
        "#);
    }

    #[test]
    fn changing_ordering_retains_insertion_order() {
        let hints = [
            HintMessage::new("last").with_ordering(HintOrdering::Last),
            HintMessage::new("first").with_ordering(HintOrdering::First),
            HintMessage::new("any"),
        ]
        .into_iter()
        .collect::<Hints<'_>>()
        .with_ordering(HintOrdering::Any);

        assert_debug_snapshot!(hints.iter().collect::<Vec<_>>(), @r#"
        [
            "last",
            "first",
            "any",
        ]
        "#);
    }

    #[test]
    fn duplicate_hints_retain_the_earliest_ordering() {
        let message = String::from("shared");
        let mut hints = Hints::from(message.as_str())
            .with_ordering(HintOrdering::Last)
            .into_owned();
        drop(message);

        hints.extend(Hints::from("first").with_ordering(HintOrdering::First));
        hints.extend(Hints::from("shared").with_ordering(HintOrdering::First));
        hints.extend(Hints::from("shared").with_ordering(HintOrdering::Last));
        hints.extend(Hints::from("any"));

        assert_snapshot!(anstream::adapter::strip_str(&hints.to_string()), @"
        hint: shared
        hint: first
        hint: any
        ");
    }

    #[test]
    fn error_with_hints_separates_hints_from_error() {
        let output = ErrorWithHints::new("error", Hints::from("fix it")).to_string();
        assert_snapshot!(anstream::adapter::strip_str(&output), @"
        error

        hint: fix it
        ");
        assert_snapshot!(ErrorWithHints::new("error", Hints::none()), @"error");
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
        write_error_chain_with_options(
            &error,
            &Hints::none(),
            ErrorOptions::default()
                .with_width_override(80)
                .with_stream(&mut output),
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
        write_error_chain_with_options(
            &error,
            &Hints::none(),
            ErrorOptions::default().with_stream(&mut output),
        )
        .unwrap();
        assert_snapshot!(format!("{output:?}"), @r#""\u{1b}[1m\u{1b}[31merror\u{1b}[39m\u{1b}[0m\u{1b}[1m:\u{1b}[0m Failed to write file\n  \u{1b}[1m\u{1b}[31mCaused by\u{1b}[39m\u{1b}[0m: Permission denied\n""#);
        let output = anstream::adapter::strip_str(&output);

        assert_snapshot!(output, @r"
        error: Failed to write file
          Caused by: Permission denied
        ");
    }

    #[test]
    fn formats_debug_error_chain() {
        #[derive(Debug, thiserror::Error)]
        #[error("inner error")]
        struct InnerError {
            code: u8,
        }

        #[derive(Debug, thiserror::Error)]
        #[error("outer error")]
        struct OuterError {
            #[source]
            source: InnerError,
        }

        let error = OuterError {
            source: InnerError { code: 42 },
        };

        assert_eq!(
            debug_error_chain(&error).to_string(),
            "0: OuterError { source: InnerError { code: 42 } }\n1: InnerError { code: 42 }"
        );
    }

    #[test]
    fn format_with_custom_level() {
        let error = anyhow!("Failed to create registry entry");
        let mut output = String::new();
        write_error_chain_with_options(
            error.as_ref(),
            &Hints::none(),
            ErrorOptions::default()
                .with_level("warning")
                .with_color(AnsiColors::Yellow)
                .with_stream(&mut output),
        )
        .unwrap();
        let output = anstream::adapter::strip_str(&output);

        assert_snapshot!(output, @"warning: Failed to create registry entry
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
        write_error_chain_with_options(
            &error,
            &Hints::none(),
            ErrorOptions::default()
                .with_width_override(50)
                .with_stream(&mut output),
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
        write_error_chain_with_options(
            &error,
            &Hints::none(),
            ErrorOptions::default()
                .with_width_override(40)
                .with_stream(&mut output),
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
        write_error_chain_with_options(
            &error,
            &Hints::none(),
            ErrorOptions::default()
                .with_width_override(60)
                .with_stream(&mut output),
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
        write_error_chain_with_options(
            &error,
            &Hints::none(),
            ErrorOptions::default()
                .with_width_override(50)
                .with_stream(&mut output),
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
        write_error_chain_with_options(
            &error,
            &Hints::none(),
            ErrorOptions::default()
                .with_width_override(50)
                .with_stream(&mut output),
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
        write_error_chain_with_options(
            err.as_ref(),
            &hints,
            ErrorOptions::default().with_stream(&mut rendered),
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
        write_error_chain_with_options(
            err.as_ref(),
            &Hints::none(),
            ErrorOptions::default().with_stream(&mut rendered),
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
