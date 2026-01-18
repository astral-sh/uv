//! Markdown test file parser.
//!
//! Parses markdown files into test definitions using pulldown-cmark.

use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use std::path::PathBuf;
use thiserror::Error;

use crate::types::{
    CodeBlockAttributes, Command, EmbeddedFile, FileSnapshot, MarkdownTest, MarkdownTestFile,
    TestConfig,
};

/// Errors that can occur during parsing.
#[derive(Debug, Error)]
pub enum ParseError {
    #[error("Invalid TOML configuration at line {line}: {message}")]
    InvalidConfig { line: usize, message: String },

    #[error("Command block missing expected output at line {line}")]
    MissingExpectedOutput { line: usize },

    #[error("File contains no tests")]
    NoTests,
}

impl MarkdownTestFile {
    /// Parse a markdown file into a test file structure.
    ///
    /// # Arguments
    ///
    /// * `path` - The file path (for error reporting)
    /// * `source` - The markdown source content
    ///
    /// # Returns
    ///
    /// A `MarkdownTestFile` containing all tests extracted from the file.
    pub fn parse(path: PathBuf, source: &str) -> Result<Self, ParseError> {
        let mut parser_state = ParserState::new(path.clone(), source);
        parser_state.parse()?;

        let tests = parser_state.finalize()?;

        Ok(Self { path, tests })
    }
}

/// Internal parser state.
struct ParserState<'a> {
    /// Source file path (reserved for future error reporting).
    #[expect(dead_code)]
    path: PathBuf,
    /// Source content.
    source: &'a str,
    /// Current line number (1-indexed).
    current_line: usize,
    /// Stack of section headers for building test names.
    header_stack: Vec<(usize, String)>, // (level, title)
    /// File-level config (applies to all tests).
    file_config: TestConfig,
    /// Current section's config override.
    section_config: Option<TestConfig>,
    /// Current section's files.
    current_files: Vec<EmbeddedFile>,
    /// Current section's commands.
    current_commands: Vec<Command>,
    /// Current section's file snapshots.
    current_file_snapshots: Vec<FileSnapshot>,
    /// Line number where current section starts.
    section_start_line: usize,
    /// Whether we've seen any headers yet.
    seen_header: bool,
    /// All completed tests.
    tests: Vec<MarkdownTest>,
}

impl<'a> ParserState<'a> {
    fn new(path: PathBuf, source: &'a str) -> Self {
        Self {
            path,
            source,
            current_line: 1,
            header_stack: Vec::new(),
            file_config: TestConfig::default(),
            section_config: None,
            current_files: Vec::new(),
            current_commands: Vec::new(),
            current_file_snapshots: Vec::new(),
            section_start_line: 1,
            seen_header: false,
            tests: Vec::new(),
        }
    }

    fn parse(&mut self) -> Result<(), ParseError> {
        let options = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH;
        let parser = Parser::new_ext(self.source, options);

        let mut in_heading = false;
        let mut current_heading_level = 0usize;
        let mut current_heading_text = String::new();
        let mut in_code_block = false;
        let mut code_block_info = String::new();
        let mut code_block_content = String::new();
        let mut code_block_start_line = 0;

        // Track line numbers by counting newlines in the source
        let mut byte_offset = 0usize;

        for (event, range) in parser.into_offset_iter() {
            // Update line number based on byte offset
            while byte_offset < range.start {
                if self.source.as_bytes().get(byte_offset) == Some(&b'\n') {
                    self.current_line += 1;
                }
                byte_offset += 1;
            }

            match event {
                Event::Start(Tag::Heading { level, .. }) => {
                    in_heading = true;
                    current_heading_level = heading_level_to_usize(level);
                    current_heading_text.clear();
                }
                Event::End(TagEnd::Heading(_)) => {
                    in_heading = false;
                    self.handle_heading(current_heading_level, &current_heading_text);
                }
                Event::Text(text) if in_heading => {
                    current_heading_text.push_str(&text);
                }
                Event::Start(Tag::CodeBlock(kind)) => {
                    in_code_block = true;
                    code_block_content.clear();
                    code_block_start_line = self.current_line;
                    code_block_info = match kind {
                        CodeBlockKind::Fenced(info) => info.to_string(),
                        CodeBlockKind::Indented => String::new(),
                    };
                }
                Event::End(TagEnd::CodeBlock) => {
                    in_code_block = false;
                    self.handle_code_block(
                        &code_block_info,
                        &code_block_content,
                        code_block_start_line,
                    )?;
                }
                Event::Text(text) if in_code_block => {
                    code_block_content.push_str(&text);
                }
                _ => {}
            }
        }

        // Finalize the last section
        self.flush_section();

        Ok(())
    }

    fn handle_heading(&mut self, level: usize, title: &str) {
        // First, flush any pending content from the previous section
        self.flush_section();

        // Pop headers that are at the same or deeper level
        while let Some((existing_level, _)) = self.header_stack.last() {
            if *existing_level >= level {
                self.header_stack.pop();
            } else {
                break;
            }
        }

        // Push the new header
        self.header_stack.push((level, title.to_string()));

        // Mark that we've seen a header
        self.seen_header = true;

        // Reset section state
        self.section_start_line = self.current_line;
        self.section_config = None;
    }

    fn handle_code_block(
        &mut self,
        info_string: &str,
        content: &str,
        line_number: usize,
    ) -> Result<(), ParseError> {
        let attrs = CodeBlockAttributes::parse(info_string);
        let content = content.trim_end_matches('\n').to_string();

        // Check if this is a config block
        if attrs.title.as_deref() == Some("mdtest.toml") {
            let config: TestConfig =
                toml::from_str(&content).map_err(|e| ParseError::InvalidConfig {
                    line: line_number,
                    message: e.to_string(),
                })?;

            if self.seen_header {
                // Config in a section - merge with file config for this section only
                self.section_config = Some(self.file_config.merge(&config));
            } else {
                // Config before any headers - this is file-level config
                self.file_config = self.file_config.merge(&config);
            }
            return Ok(());
        }

        // Check if this is a command block (starts with `$ `)
        if content.starts_with("$ ") {
            let command = parse_command_block(&content, line_number)?;
            self.current_commands.push(command);
            return Ok(());
        }

        // Check if this is a file snapshot
        if attrs.snapshot {
            if let Some(title) = attrs.title {
                self.current_file_snapshots.push(FileSnapshot {
                    path: PathBuf::from(title),
                    expected_content: content,
                    line_number,
                });
            }
            return Ok(());
        }

        // Check if this is an embedded file
        if let Some(title) = attrs.title {
            self.current_files.push(EmbeddedFile {
                path: PathBuf::from(title),
                content,
                line_number,
            });
        }

        Ok(())
    }

    fn flush_section(&mut self) {
        // Only create a test if we have commands or file snapshots
        if !self.current_commands.is_empty() || !self.current_file_snapshots.is_empty() {
            // Build the test name from the header hierarchy
            let name = self
                .header_stack
                .iter()
                .map(|(_, title)| title.as_str())
                .collect::<Vec<_>>()
                .join(" - ");

            // Use section config if set, otherwise use file config
            let config = self
                .section_config
                .clone()
                .unwrap_or_else(|| self.file_config.clone());

            self.tests.push(MarkdownTest {
                name,
                config,
                files: std::mem::take(&mut self.current_files),
                commands: std::mem::take(&mut self.current_commands),
                file_snapshots: std::mem::take(&mut self.current_file_snapshots),
                line_number: self.section_start_line,
            });
        }

        // Clear current section state
        self.current_files.clear();
        self.current_commands.clear();
        self.current_file_snapshots.clear();
        self.section_config = None;
    }

    fn finalize(self) -> Result<Vec<MarkdownTest>, ParseError> {
        if self.tests.is_empty() {
            return Err(ParseError::NoTests);
        }

        Ok(self.tests)
    }
}

/// Parse a command block into a Command structure.
fn parse_command_block(content: &str, line_number: usize) -> Result<Command, ParseError> {
    let lines: Vec<&str> = content.lines().collect();

    if lines.is_empty() {
        return Err(ParseError::MissingExpectedOutput { line: line_number });
    }

    // First line is the command (starting with `$ `)
    let command_line = lines[0];
    let command = command_line
        .strip_prefix("$ ")
        .unwrap_or(command_line)
        .to_string();

    // Rest is the expected output
    let expected_output = if lines.len() > 1 {
        lines[1..].join("\n")
    } else {
        String::new()
    };

    Ok(Command {
        command,
        expected_output,
        line_number,
    })
}

fn heading_level_to_usize(level: HeadingLevel) -> usize {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_test() {
        let source = r#"
# Lock

Tests for lock command.

## Basic locking

```toml title="pyproject.toml"
[project]
name = "test"
version = "0.1.0"
```

```
$ uv lock
success: true
exit_code: 0
----- stdout -----

----- stderr -----
Resolved 1 package in [TIME]
```
"#;

        let result = MarkdownTestFile::parse(PathBuf::from("test.md"), source).unwrap();
        assert_eq!(result.tests.len(), 1);

        let test = &result.tests[0];
        assert_eq!(test.name, "Lock - Basic locking");
        assert_eq!(test.files.len(), 1);
        assert_eq!(test.files[0].path, PathBuf::from("pyproject.toml"));
        assert_eq!(test.commands.len(), 1);
        assert_eq!(test.commands[0].command, "uv lock");
    }

    #[test]
    fn test_parse_with_file_level_config() {
        let source = r#"
```toml title="mdtest.toml"
[environment]
python-version = "3.12"
exclude-newer = "2024-03-25T00:00:00Z"
```

# Tests

## Test one

```toml title="pyproject.toml"
[project]
name = "test"
```

```
$ uv lock
success: true
exit_code: 0
----- stdout -----

----- stderr -----
Done
```

## Test two

```toml title="pyproject.toml"
[project]
name = "test2"
```

```
$ uv lock
success: true
exit_code: 0
----- stdout -----

----- stderr -----
Done
```
"#;

        let result = MarkdownTestFile::parse(PathBuf::from("test.md"), source).unwrap();
        assert_eq!(result.tests.len(), 2);

        // Both tests should have the file-level config
        assert_eq!(
            result.tests[0].config.environment.python_version.as_deref(),
            Some("3.12")
        );
        assert_eq!(
            result.tests[1].config.environment.python_version.as_deref(),
            Some("3.12")
        );
    }

    #[test]
    fn test_parse_sections_are_independent() {
        let source = r#"
# Tests

## Test A

```toml title="a.toml"
content = "a"
```

```
$ uv lock
success: true
exit_code: 0
----- stdout -----

----- stderr -----
Done
```

## Test B

```toml title="b.toml"
content = "b"
```

```
$ uv lock
success: true
exit_code: 0
----- stdout -----

----- stderr -----
Done
```
"#;

        let result = MarkdownTestFile::parse(PathBuf::from("test.md"), source).unwrap();
        assert_eq!(result.tests.len(), 2);

        // Test A should only have a.toml
        assert_eq!(result.tests[0].files.len(), 1);
        assert_eq!(result.tests[0].files[0].path, PathBuf::from("a.toml"));

        // Test B should only have b.toml (no inheritance from A)
        assert_eq!(result.tests[1].files.len(), 1);
        assert_eq!(result.tests[1].files[0].path, PathBuf::from("b.toml"));
    }

    #[test]
    fn test_parse_with_file_snapshot() {
        let source = r#"
# Lock

## With snapshot

```toml title="pyproject.toml"
[project]
name = "test"
```

```
$ uv lock
success: true
exit_code: 0
----- stdout -----

----- stderr -----
Done
```

```toml title="uv.lock" snapshot=true
version = 1
requires-python = ">=3.12"
```
"#;

        let result = MarkdownTestFile::parse(PathBuf::from("test.md"), source).unwrap();
        assert_eq!(result.tests.len(), 1);

        let test = &result.tests[0];
        assert_eq!(test.file_snapshots.len(), 1);
        assert_eq!(test.file_snapshots[0].path, PathBuf::from("uv.lock"));
    }

    #[test]
    fn test_parse_code_block_attributes() {
        let attrs = CodeBlockAttributes::parse("toml title=\"pyproject.toml\"");
        assert_eq!(attrs.language.as_deref(), Some("toml"));
        assert_eq!(attrs.title.as_deref(), Some("pyproject.toml"));
        assert!(!attrs.snapshot);

        let attrs = CodeBlockAttributes::parse("toml title=\"uv.lock\" snapshot=true");
        assert!(attrs.snapshot);
    }
}
