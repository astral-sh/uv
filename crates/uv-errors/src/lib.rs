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

#[cfg(test)]
mod tests {
    use super::{ErrorWithHints, HintPrefix, Hints};

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
}
