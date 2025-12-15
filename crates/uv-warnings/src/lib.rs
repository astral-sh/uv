use std::error::Error;
use std::iter;
use std::sync::atomic::AtomicBool;
use std::sync::{LazyLock, Mutex};

// macro hygiene: The user might not have direct dependencies on those crates
#[doc(hidden)]
pub use anstream;
#[doc(hidden)]
pub use owo_colors;
use owo_colors::DynColor;
use rustc_hash::FxHashSet;
use uv_static::EnvVars;

/// Whether user-facing warnings are enabled.
pub static ENABLED: AtomicBool = AtomicBool::new(false);

/// Enable user-facing warnings.
pub fn enable() {
    ENABLED.store(true, std::sync::atomic::Ordering::Relaxed);
}

/// Disable user-facing warnings.
pub fn disable() {
    ENABLED.store(false, std::sync::atomic::Ordering::Relaxed);
}

/// Warn a user, if warnings are enabled.
#[macro_export]
macro_rules! warn_user {
    ($($arg:tt)*) => {{
        use $crate::anstream::eprintln;
        use $crate::owo_colors::OwoColorize;

        if $crate::ENABLED.load(std::sync::atomic::Ordering::Relaxed) {
            let message = format!("{}", format_args!($($arg)*));
            let formatted = message.bold();
            eprintln!("{}{} {formatted}", "warning".yellow().bold(), ":".bold());
        }
    }};
}

pub static WARNINGS: LazyLock<Mutex<FxHashSet<String>>> = LazyLock::new(Mutex::default);

/// Warn a user once, if warnings are enabled, with uniqueness determined by the content of the
/// message.
#[macro_export]
macro_rules! warn_user_once {
    ($($arg:tt)*) => {{
        use $crate::anstream::eprintln;
        use $crate::owo_colors::OwoColorize;

        if $crate::ENABLED.load(std::sync::atomic::Ordering::Relaxed) {
            if let Ok(mut states) = $crate::WARNINGS.lock() {
                let message = format!("{}", format_args!($($arg)*));
                if states.insert(message.clone()) {
                    eprintln!("{}{} {}", "warning".yellow().bold(), ":".bold(), message.bold());
                }
            }
        }
    }};
}

/// Checks if line wrapping should be enabled.
/// Returns false if `UV_NO_WRAP` is set.
fn should_wrap_lines() -> bool {
    std::env::var_os(EnvVars::UV_NO_WRAP).is_none()
}

/// Gets the terminal width for wrapping.
/// Checks override, then COLUMNS env var, then terminal size detection.
/// Returns `None` if width cannot be determined (no wrapping should occur).
fn get_wrap_width(width_override: Option<usize>) -> Option<usize> {
    if !should_wrap_lines() {
        return None;
    }

    // Use override if provided (for testing)
    if let Some(width) = width_override {
        return Some(width);
    }

    // Check COLUMNS environment variable
    if let Ok(cols) = std::env::var(EnvVars::COLUMNS) {
        if let Ok(width) = cols.parse::<usize>() {
            return Some(width);
        }
    }

    // Try to detect terminal width
    if let Some((terminal_size::Width(width), _)) = terminal_size::terminal_size() {
        return Some(width as usize);
    }

    // No width detected - don't wrap
    None
}

/// Wraps text at word boundaries with proper indentation.
///
/// Based on miette's `wrap()` implementation from:
/// <https://github.com/zkat/miette/blob/v7.2.0/src/handlers/graphical.rs#L876-L909>
fn wrap_text(
    text: &str,
    width: Option<usize>,
    initial_indent: &str,
    subsequent_indent: &str,
) -> String {
    if let Some(width) = width {
        let options = textwrap::Options::new(width)
            .initial_indent(initial_indent)
            .subsequent_indent(subsequent_indent)
            .break_words(false)
            .word_separator(textwrap::WordSeparator::AsciiSpace)
            .word_splitter(textwrap::WordSplitter::NoHyphenation);

        textwrap::fill(text, options)
    } else {
        // If not wrapping, apply indentation while preserving line breaks
        let mut result = String::with_capacity(2 * text.len());

        for (idx, line) in text.split_terminator('\n').enumerate() {
            if idx == 0 {
                result.push_str(initial_indent);
            } else {
                result.push('\n');
                // Don't add indent to empty lines (avoid trailing whitespace)
                if !line.is_empty() {
                    result.push_str(subsequent_indent);
                }
            }
            result.push_str(line);
        }

        result
    }
}

/// Format an error or warning chain with custom options.
///
/// # Example
///
/// ```text
/// error: Failed to install app
///   Caused By: Failed to install dependency
///   Caused By: Error writing failed `/home/ferris/deps/foo`: Permission denied
/// ```
///
/// ```text
/// warning: Failed to create registry entry for Python 3.12
///   Caused By: Security policy forbids chaining registry entries
/// ```
pub fn write_error_chain_with_options(
    err: &dyn Error,
    stream: impl std::fmt::Write,
    level: impl AsRef<str>,
    color: impl DynColor + Copy,
) -> std::fmt::Result {
    write_error_chain_with_hints(err, stream, level, color, std::iter::empty::<&str>(), None)
}

/// Format an error chain with hints appended at the end.
///
/// Hints are displayed after the error chain at the top level (no indentation).
/// Each hint should already include its own "hint:" prefix.
///
/// # Example
///
/// ```text
/// error: No solution found when resolving dependencies:
///   Caused by: Because iniconfig was not found...
///
/// hint: Packages were unavailable because the network was disabled.
/// ```
pub fn write_error_chain_with_hints<'a>(
    err: &dyn Error,
    mut stream: impl std::fmt::Write,
    level: impl AsRef<str>,
    color: impl DynColor + Copy,
    hints: impl IntoIterator<Item = impl std::fmt::Display + 'a>,
    width_override: Option<usize>,
) -> std::fmt::Result {
    use owo_colors::OwoColorize;

    let width = get_wrap_width(width_override);

    // Write main error message with owo-colors styling
    let main_msg = err.to_string();
    let wrapped_main = wrap_text(&main_msg, width, "", "");
    writeln!(
        &mut stream,
        "{}{} {}",
        level.as_ref().color(color).bold(),
        ":".bold(),
        wrapped_main.trim()
    )?;

    // Write cause chain with styling
    for source in iter::successors(err.source(), |&err| err.source()) {
        let msg = source.to_string();
        let padding = "  ";
        let cause = "Caused by";
        let child_padding = " ".repeat(padding.len() + cause.len() + 2);

        // Wrap the message with proper indentation for continuation lines
        let wrapped = wrap_text(&msg, width, "", &child_padding);

        // Split wrapped output and apply coloring to "Caused by:" prefix
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
                let line = line.trim_end();
                if line.is_empty() {
                    // Avoid showing indents on empty lines
                    writeln!(&mut stream)?;
                } else {
                    writeln!(&mut stream, "{line}")?;
                }
            }
        }
    }

    // Write hints at top-level (no indentation)
    for hint in hints {
        writeln!(&mut stream)?;
        writeln!(&mut stream, "{hint}")?;
    }

    Ok(())
}

/// Format an error chain with default options (error level, red color, auto-detected width).
pub fn write_error_chain(err: &dyn Error, stream: impl std::fmt::Write) -> std::fmt::Result {
    write_error_chain_with_options(err, stream, "error", owo_colors::AnsiColors::Red)
}

/// Format a warning chain (warning level, yellow color, auto-detected width).
pub fn write_warning_chain(err: &dyn Error, stream: impl std::fmt::Write) -> std::fmt::Result {
    write_error_chain_with_options(err, stream, "warning", owo_colors::AnsiColors::Yellow)
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::anyhow;
    use indoc::indoc;
    use insta::assert_snapshot;
    use owo_colors::AnsiColors;

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
        write_error_chain_with_hints(
            &error,
            &mut output,
            "error",
            AnsiColors::Red,
            std::iter::empty::<&str>(),
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
        write_error_chain_with_options(&error, &mut output, "error", AnsiColors::Red).unwrap();
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
        write_error_chain_with_hints(
            &error,
            &mut output,
            "error",
            AnsiColors::Red,
            std::iter::empty::<&str>(),
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
        write_error_chain_with_hints(
            &error,
            &mut output,
            "error",
            AnsiColors::Red,
            std::iter::empty::<&str>(),
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
        write_error_chain_with_hints(
            &error,
            &mut output,
            "error",
            AnsiColors::Red,
            std::iter::empty::<&str>(),
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
    fn test_wrap_only_on_ascii_space() {
        #[derive(Debug, thiserror::Error)]
        #[error("Path /usr/local/lib/python3.12/site-packages not found in filesystem hierarchy")]
        struct SpecialChars;

        let error = SpecialChars;
        let mut output = String::new();
        write_error_chain_with_hints(
            &error,
            &mut output,
            "error",
            AnsiColors::Red,
            std::iter::empty::<&str>(),
            Some(50),
        )
        .unwrap();
        let output = anstream::adapter::strip_str(&output);
        assert_snapshot!(output, @r"
        error: Path /usr/local/lib/python3.12/site-packages not
        found in filesystem hierarchy
        ");
    }

    #[test]
    fn format_error_with_hints() {
        let err = anyhow!("Because iniconfig was not found in the package registry")
            .context("No solution found when resolving dependencies:");

        let hints = vec![
            "hint: Packages were unavailable because the network was disabled.",
            "hint: Try running without --offline.",
        ];

        let mut rendered = String::new();
        write_error_chain_with_hints(
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
        error: No solution found when resolving dependencies:
          Caused by: Because iniconfig was not found in the package registry

        hint: Packages were unavailable because the network was disabled.

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
        write_error_chain(err.as_ref(), &mut rendered).unwrap();
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
