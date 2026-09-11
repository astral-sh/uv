use std::borrow::Cow;
use std::error::Error;
use std::fmt::{self, Write};

use owo_colors::OwoColorize;

use crate::line_wrap::wrap_text;
use crate::source::SourceSnippet;

/// User-facing presentation data for one error in a source chain.
///
/// This does not replace the error or its sources. Unrecognized error types continue to use
/// their [`fmt::Display`] implementation.
#[derive(Default)]
pub struct Diagnostic<'a> {
    pub(crate) message: Option<Cow<'a, str>>,
    pub(crate) snippets: Vec<SourceSnippet>,
    pub(crate) info: Vec<Info<'a>>,
    pub(crate) source: Option<Box<Self>>,
}

impl<'a> Diagnostic<'a> {
    /// Override the displayed message for this error.
    pub fn new(message: impl Into<Cow<'a, str>>) -> Self {
        Self {
            message: Some(message.into()),
            snippets: Vec::new(),
            info: Vec::new(),
            source: None,
        }
    }

    /// Attach additional context to this error.
    #[cfg(test)]
    #[must_use]
    pub(super) fn with_info(mut self, info: Info<'a>) -> Self {
        self.info.push(info);
        self
    }

    /// Attach a source location to this error.
    #[must_use]
    pub fn with_snippet(mut self, snippet: SourceSnippet) -> Self {
        self.snippets.push(snippet);
        self
    }

    /// Supply presentation data for the next actual [`Error::source`] node.
    ///
    /// This does not add, replace, or remove an error from the source chain.
    #[must_use]
    pub fn with_source(mut self, source: Self) -> Self {
        self.source = Some(Box::new(source));
        self
    }
}

/// Additional context, rather than a cause or an actionable hint.
pub(super) struct Info<'a> {
    message: Cow<'a, str>,
    details: Option<Cow<'a, str>>,
}

#[cfg(test)]
impl<'a> Info<'a> {
    /// Create an informational statement.
    pub(super) fn new(message: impl Into<Cow<'a, str>>) -> Self {
        Self {
            message: message.into(),
            details: None,
        }
    }

    /// Attach a block of text, retaining its authored line breaks and indentation.
    /// Terminal control characters are escaped when the block is rendered.
    #[must_use]
    pub(super) fn with_details(mut self, details: impl Into<Cow<'a, str>>) -> Self {
        self.details = Some(details.into());
        self
    }
}

/// Resolve presentation data for a concrete error type.
pub type DiagnosticFn = for<'a> fn(&'a (dyn Error + 'static)) -> Option<Diagnostic<'a>>;

pub(crate) fn write_info(
    stream: &mut impl Write,
    info: &[Info<'_>],
    width: Option<usize>,
) -> fmt::Result {
    for info in info {
        let message = wrap_text(
            &info.message,
            width.map(|width| width.saturating_sub(8)),
            "",
            "",
            "",
        );
        let mut lines = message.lines();
        writeln!(
            stream,
            "  {}{} {}",
            "info".cyan().bold(),
            ":".bold(),
            lines.next().unwrap_or_default().trim(),
        )?;
        for line in lines {
            if line.trim().is_empty() {
                writeln!(stream)?;
            } else {
                writeln!(stream, "        {line}")?;
            }
        }
        if let Some(details) = info
            .details
            .as_deref()
            .filter(|details| !details.is_empty())
        {
            let details = normalize_details(details);
            let details = wrap_text(
                &details,
                width.map(|width| width.saturating_sub(6)),
                "",
                "",
                "",
            );
            writeln!(stream, "    {}", "|".cyan().bold())?;
            for line in details.lines() {
                if line.trim().is_empty() {
                    writeln!(stream, "    {}", "|".cyan().bold())?;
                } else {
                    writeln!(stream, "    {} {line}", "|".cyan().bold())?;
                }
            }
            writeln!(stream, "    {}", "|".cyan().bold())?;
        }
    }
    Ok(())
}

/// Keep untrusted text inside its diagnostic gutter, including on ANSI-capable terminals.
pub(crate) fn normalize_details(details: &str) -> Cow<'_, str> {
    if !details.chars().any(|character| {
        (character.is_control() && character != '\n') || is_layout_control(character)
    }) {
        return Cow::Borrowed(details);
    }

    let mut normalized = String::with_capacity(details.len());
    let mut characters = details.chars().peekable();
    while let Some(character) = characters.next() {
        match character {
            '\r' if characters.peek() == Some(&'\n') => {
                characters.next();
                normalized.push('\n');
            }
            '\n' => normalized.push('\n'),
            '\t' => normalized.push_str("    "),
            character if character.is_control() => normalized.extend(character.escape_debug()),
            character if is_layout_control(character) => {
                normalized.extend(character.escape_unicode());
            }
            character => normalized.push(character),
        }
    }
    Cow::Owned(normalized)
}

pub(crate) fn is_layout_control(character: char) -> bool {
    matches!(
        character,
        '\u{061c}'
            | '\u{200e}'..='\u{200f}'
            | '\u{2028}'..='\u{202e}'
            | '\u{2066}'..='\u{2069}'
    )
}
