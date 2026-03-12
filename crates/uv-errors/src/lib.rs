use std::borrow::Cow;
use std::fmt;

use owo_colors::OwoColorize;

/// An error that may carry user-facing hints.
///
/// Implement this on error types that want to surface contextual suggestions
/// (e.g., "try `--prerelease=allow`") to the diagnostics layer. Hints are
/// rendered after the error output, each prefixed with `hint:`.
pub trait Hint {
    /// Return any hints associated with this error.
    fn hints(&self) -> Vec<Cow<'_, str>> {
        Vec::new()
    }
}

/// A styled `hint:` prefix for use in user-facing messages.
pub struct HintPrefix;

impl fmt::Display for HintPrefix {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", "hint".bold().cyan(), ":".bold())
    }
}

/// Write formatted hints to the given [`fmt::Write`] destination.
///
/// Each hint is prefixed with the styled `hint:` label.
pub fn write_hints<'a>(
    writer: &mut impl fmt::Write,
    hints: impl IntoIterator<Item = &'a (impl fmt::Display + 'a)>,
) {
    for hint in hints {
        let _ = write!(writer, "\n{HintPrefix} {hint}");
    }
}
