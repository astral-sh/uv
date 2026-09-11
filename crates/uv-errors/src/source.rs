use std::fmt::{self, Write};
use std::ops::Range;
use std::sync::Arc;

use owo_colors::{DynColor, OwoColorize};
use unicode_width::UnicodeWidthStr;

use crate::diagnostic::{is_layout_control, normalize_details};

/// An immutable source snapshot captured when an error is produced.
///
/// Spans refer to UTF-8 byte offsets in the decoded text. The renderer never reopens the file.
#[derive(Clone)]
pub struct SourceFile {
    inner: Arc<SourceFileInner>,
}

struct SourceFileInner {
    name: Arc<str>,
    text: Arc<str>,
}

impl SourceFile {
    /// Retain the display name and decoded source text.
    ///
    /// The name must be safe to show, for example a local path or a redacted URL.
    pub fn new(name: impl Into<Arc<str>>, text: impl Into<Arc<str>>) -> Self {
        Self {
            inner: Arc::new(SourceFileInner {
                name: name.into(),
                text: text.into(),
            }),
        }
    }

    /// The user-facing name of this source.
    pub fn name(&self) -> &str {
        &self.inner.name
    }

    /// The exact decoded text used by the parser.
    pub fn text(&self) -> &str {
        &self.inner.text
    }
}

impl fmt::Debug for SourceFile {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Configuration files can contain credentials; debug output must not dump the snapshot.
        formatter
            .debug_struct("SourceFile")
            .field("name", &self.name())
            .field("len", &self.text().len())
            .finish()
    }
}

/// The first physical source line touched by a parser's byte span.
///
/// Only that line is shown, since surrounding configuration may contain credentials. This does
/// not redact values on the selected line. Multiline spans are clipped to its visible text.
#[derive(Clone, Debug)]
pub struct SourceSnippet {
    source: SourceFile,
    line: Range<usize>,
    annotation: Range<usize>,
    line_number: usize,
    column: usize,
}

impl SourceSnippet {
    /// Select a source line, or return `None` for an invalid UTF-8 byte span.
    pub fn new(source: SourceFile, span: Range<usize>) -> Option<Self> {
        let text = source.text();
        text.get(span.clone())?;
        let before = text.get(..span.start)?;
        let after = text.get(span.start..)?;
        let start = before.rfind('\n').map_or(0, |index| index + 1);
        let newline = after.find('\n').map(|index| span.start + index);
        let mut end = newline.unwrap_or(text.len());
        if newline.is_some() && text.get(start..end)?.ends_with('\r') {
            end -= 1;
        }
        let annotation_start = span.start.min(end) - start;
        let annotation_end = span.end.min(end) - start;
        let line_number = before.bytes().filter(|byte| *byte == b'\n').count() + 1;
        let column = text.get(start..span.start.min(end))?.chars().count() + 1;
        Some(Self {
            source,
            line: start..end,
            annotation: annotation_start..annotation_end,
            line_number,
            column,
        })
    }
}

pub(crate) fn write_snippets<C: DynColor + Copy>(
    stream: &mut impl Write,
    snippets: &[SourceSnippet],
    color: C,
) -> fmt::Result {
    for snippet in snippets {
        let line = &snippet.source.text()[snippet.line.clone()];
        let prefix = normalize_details(&line[..snippet.annotation.start]);
        let through_annotation = normalize_details(&line[..snippet.annotation.end]);
        let offset = UnicodeWidthStr::width(prefix.as_ref());
        let length = UnicodeWidthStr::width(through_annotation.as_ref())
            .saturating_sub(offset)
            .max(1);
        let line = normalize_details(line);
        let name = normalize_name(snippet.source.name());
        let line_number = snippet.line_number.to_string();
        let gutter = " ".repeat(line_number.len() + 3);
        writeln!(
            stream,
            "{}{} {name}:{}:{}",
            " ".repeat(line_number.len() + 2),
            "-->".cyan().bold(),
            snippet.line_number,
            snippet.column,
        )?;
        writeln!(stream, "{gutter}{}", "|".cyan().bold())?;
        if line.is_empty() {
            writeln!(
                stream,
                "  {} {}",
                line_number.cyan().bold(),
                "|".cyan().bold()
            )?;
        } else {
            writeln!(
                stream,
                "  {} {} {line}",
                line_number.cyan().bold(),
                "|".cyan().bold(),
            )?;
        }
        writeln!(
            stream,
            "{gutter}{} {}{}",
            "|".cyan().bold(),
            " ".repeat(offset),
            "^".repeat(length).color(color).bold(),
        )?;
    }
    Ok(())
}

/// Keep a source name on one terminal line without obscuring control characters.
fn normalize_name(name: &str) -> String {
    let mut normalized = String::with_capacity(name.len());
    for character in name.chars() {
        if character.is_control() {
            normalized.extend(character.escape_debug());
        } else if is_layout_control(character) {
            normalized.extend(character.escape_unicode());
        } else {
            normalized.push(character);
        }
    }
    normalized
}

#[cfg(test)]
mod tests {
    use insta::{assert_debug_snapshot, assert_snapshot};
    use owo_colors::AnsiColors;

    use super::{SourceFile, SourceSnippet, write_snippets};

    fn render(source: &str, span: std::ops::Range<usize>) -> String {
        let snippet = SourceSnippet::new(SourceFile::new("pyproject.toml", source), span)
            .expect("valid source span in test input");
        let mut output = String::new();
        write_snippets(&mut output, &[snippet], AnsiColors::Red)
            .expect("writing to a string should not fail");
        anstream::adapter::strip_str(&output).to_string()
    }

    #[test]
    fn single_source_line() {
        let source = "token = 'first-secret'\r\nversion = 42\r\nother = 'second-secret'\r\n";
        let start = source.find("42").expect("invalid version in test input");
        assert_snapshot!(render(source, start..start + 2), @"
           --> pyproject.toml:2:11
            |
          2 | version = 42
            |           ^^
        ");
    }

    #[test]
    fn multiline_and_eof_spans() {
        let source = "items = [\n  'first',\n]\n";
        assert_snapshot!(render(source, 8..source.len()), @"
           --> pyproject.toml:1:9
            |
          1 | items = [
            |         ^
        ");
        assert_snapshot!(render(source, source.len()..source.len()), @"
           --> pyproject.toml:4:1
            |
          4 |
            | ^
        ");
    }

    #[test]
    fn untrusted_source_text() {
        let source = "\tname = 'café 界'\u{1b}[31m\u{202e}value";
        let start = source.find("value").expect("highlight in test input");
        assert_snapshot!(render(source, start..source.len()), @r"
         --> pyproject.toml:1:23
          |
        1 |     name = 'café 界'\u{1b}[31m\u{202e}value
          |                                       ^^^^^
        ");
    }

    #[test]
    fn invalid_source_spans() {
        let source = SourceFile::new("pyproject.toml", "café");
        assert!(SourceSnippet::new(source.clone(), 3..4).is_none());
        assert!(SourceSnippet::new(source.clone(), 4..5).is_none());
        assert!(SourceSnippet::new(source, std::ops::Range { start: 2, end: 1 }).is_none());
    }

    #[test]
    fn source_names_and_debug_output_are_safe() {
        let source = SourceFile::new("uv\n\u{1b}[31m\u{202e}.toml", "token = 'secret'");
        assert_debug_snapshot!(source, @r#"
        SourceFile {
            name: "uv\n\u{1b}[31m\u{202e}.toml",
            len: 16,
        }
        "#);
        let snippet = SourceSnippet::new(source, 8..16).expect("valid source span in test input");
        let mut output = String::new();
        write_snippets(&mut output, &[snippet], AnsiColors::Red)
            .expect("writing to a string should not fail");
        assert_snapshot!(anstream::adapter::strip_str(&output), @r"
           --> uv\n\u{1b}[31m\u{202e}.toml:1:9
            |
          1 | token = 'secret'
            |         ^^^^^^^^
        ");
    }
}
