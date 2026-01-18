//! Markdown test file parser.
//!
//! Parses markdown files into test definitions using pulldown-cmark.

use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use std::path::PathBuf;
use thiserror::Error;

use crate::types::{
    CodeBlockAttributes, Command, ContentAssertion, EmbeddedFile, FileSnapshot, MarkdownTest,
    MarkdownTestFile, TestConfig, TestStep, TreeCreation, TreeEntry, TreeSnapshot,
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

    #[error(
        "Multiple title headers found at line {line}. Only one level-1 header (`#`) is allowed per file."
    )]
    MultipleTitles { line: usize },
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
    /// Current section's steps (files, trees, commands, snapshots in document order).
    current_steps: Vec<TestStep>,
    /// Line number where current section starts.
    section_start_line: usize,
    /// Whether we've seen a section header (level 2 or deeper).
    /// Level 1 headers are treated as document titles, not test sections.
    seen_section_header: bool,
    /// Whether we've seen the document title (level 1 header).
    seen_title: bool,
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
            current_steps: Vec::new(),
            section_start_line: 1,
            seen_section_header: false,
            seen_title: false,
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
                    self.handle_heading(current_heading_level, &current_heading_text)?;
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

    fn handle_heading(&mut self, level: usize, title: &str) -> Result<(), ParseError> {
        // First, flush any pending content from the previous section
        self.flush_section();

        // Check for multiple title headers
        if level == 1 {
            if self.seen_title {
                return Err(ParseError::MultipleTitles {
                    line: self.current_line,
                });
            }
            self.seen_title = true;
        }

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

        // Mark that we've seen a section header (level 2 or deeper).
        // Level 1 headers are document titles, not test sections.
        if level >= 2 {
            self.seen_section_header = true;
        }

        // Reset section state
        self.section_start_line = self.current_line;
        self.section_config = None;

        Ok(())
    }

    fn handle_code_block(
        &mut self,
        info_string: &str,
        content: &str,
        line_number: usize,
    ) -> Result<(), ParseError> {
        let attrs = CodeBlockAttributes::parse(info_string);
        let content = content.trim_end_matches('\n').to_string();

        // Check if this is a config block (either via title attribute or # mdtest marker)
        let is_config_by_title = attrs.title.as_deref() == Some("mdtest.toml");
        let mdtest_content = extract_mdtest_content(&content);

        if is_config_by_title || mdtest_content.is_some() {
            let config_content = mdtest_content.as_deref().unwrap_or(&content);
            let config: TestConfig =
                toml::from_str(config_content).map_err(|e| ParseError::InvalidConfig {
                    line: line_number,
                    message: e.to_string(),
                })?;

            if self.seen_section_header {
                // Config in a section - merge with file config for this section only
                self.section_config = Some(self.file_config.merge(&config));
            } else {
                // Config before any section headers (level 2+) - this is file-level config
                self.file_config = self.file_config.merge(&config);
            }
            return Ok(());
        }

        // Extract title from content (# file: prefix) if not in attributes
        let (title_from_content, content) = extract_title_from_content(&content);
        let title = attrs.title.or(title_from_content);

        // Check if this is a command block (starts with `$ `)
        if content.starts_with("$ ") {
            let working_dir = attrs.working_dir.map(PathBuf::from);
            let command = parse_command_block(&content, line_number, working_dir)?;
            self.current_steps.push(TestStep::RunCommand(command));
            return Ok(());
        }

        // Check if this is a tree block
        if attrs.language.as_deref() == Some("tree") {
            if attrs.create {
                // Parse tree content into entries for creation
                let entries = parse_tree_content(&content);
                self.current_steps.push(TestStep::CreateTree(TreeCreation {
                    entries,
                    line_number,
                }));
            } else {
                // Regular tree snapshot for verification
                self.current_steps
                    .push(TestStep::CheckTreeSnapshot(TreeSnapshot {
                        expected_content: content,
                        depth: attrs.depth,
                        line_number,
                    }));
            }
            return Ok(());
        }

        // Check if this is a content assertion
        if let Some(assert_kind) = attrs.assert {
            if let Some(title) = attrs.title {
                self.current_steps
                    .push(TestStep::CheckContentAssertion(ContentAssertion {
                        path: PathBuf::from(title),
                        kind: assert_kind,
                        expected: content,
                        line_number,
                    }));
            }
            return Ok(());
        }

        // Check if this is a file snapshot
        if attrs.snapshot {
            if let Some(title) = attrs.title {
                self.current_steps
                    .push(TestStep::CheckFileSnapshot(FileSnapshot {
                        path: PathBuf::from(title),
                        expected_content: content,
                        line_number,
                    }));
            }
            return Ok(());
        }

        // Check if this is an embedded file
        if let Some(title) = attrs.title {
            self.current_steps.push(TestStep::WriteFile(EmbeddedFile {
                path: PathBuf::from(title),
                content,
                line_number,
            }));
        }

        Ok(())
    }

    fn flush_section(&mut self) {
        // Only create a test if we have steps that require execution or verification
        let has_executable_content = self.current_steps.iter().any(|step| {
            matches!(
                step,
                TestStep::RunCommand(_)
                    | TestStep::CheckFileSnapshot(_)
                    | TestStep::CheckContentAssertion(_)
                    | TestStep::CheckTreeSnapshot(_)
            )
        });

        if has_executable_content {
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
                steps: std::mem::take(&mut self.current_steps),
                line_number: self.section_start_line,
            });
        }

        // Clear current section state
        self.current_steps.clear();
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
fn parse_command_block(
    content: &str,
    line_number: usize,
    working_dir: Option<PathBuf>,
) -> Result<Command, ParseError> {
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
        working_dir,
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

/// Extract title from content that starts with `# file: filename`.
///
/// Returns the extracted title (if any) and the remaining content with
/// the title line and optional blank line removed.
fn extract_title_from_content(content: &str) -> (Option<String>, String) {
    let lines: Vec<&str> = content.lines().collect();
    if let Some(first_line) = lines.first() {
        if let Some(filename) = first_line.strip_prefix("# file: ") {
            // Skip title line and optional blank line after it
            let start = if lines.get(1).is_some_and(|l| l.is_empty()) {
                2
            } else {
                1
            };
            let remaining = lines[start..].join("\n");
            return (Some(filename.to_string()), remaining);
        }
    }
    (None, content.to_string())
}

/// Check if content starts with `# mdtest` marker for config blocks.
///
/// Returns the remaining content with the marker line and optional blank line removed.
fn extract_mdtest_content(content: &str) -> Option<String> {
    let lines: Vec<&str> = content.lines().collect();
    if let Some(first_line) = lines.first() {
        if first_line.trim() == "# mdtest" {
            // Skip marker line and optional blank line after it
            let start = if lines.get(1).is_some_and(|l| l.is_empty()) {
                2
            } else {
                1
            };
            return Some(lines[start..].join("\n"));
        }
    }
    None
}

/// Parse tree content into a list of tree entries.
///
/// Parses content like:
/// ```text
/// .
/// ├── packages/
/// │   ├── alpha/
/// │   └── beta/
/// └── shared -> packages/alpha
/// ```
///
/// Into a list of `TreeEntry` items that can be created.
fn parse_tree_content(content: &str) -> Vec<TreeEntry> {
    let mut entries = Vec::new();
    let mut path_stack: Vec<String> = Vec::new();

    for line in content.lines() {
        // Skip empty lines and the root "." line
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed == "." {
            continue;
        }

        // Calculate depth by looking for tree drawing characters
        // Each level of indentation is 4 characters: "│   " or "    "
        // The actual entry starts after "├── " or "└── "
        let (depth, name_part) = parse_tree_line(line);

        if name_part.is_empty() {
            continue;
        }

        // Trim the path stack to the current depth
        path_stack.truncate(depth);

        // Check if this is a symlink (contains " -> ")
        if let Some((name, target)) = name_part.split_once(" -> ") {
            let name = name.trim_end_matches('/');
            let full_path: PathBuf = path_stack.iter().collect::<PathBuf>().join(name);
            entries.push(TreeEntry::Symlink {
                path: full_path,
                target: PathBuf::from(target),
            });
            // Symlinks don't get added to path stack (we don't recurse into them)
        } else if name_part.ends_with('/') {
            // This is a directory
            let name = name_part.trim_end_matches('/');
            path_stack.push(name.to_string());
            let full_path: PathBuf = path_stack.iter().collect();
            entries.push(TreeEntry::Directory { path: full_path });
        } else {
            // This is a file
            let full_path: PathBuf = path_stack.iter().collect::<PathBuf>().join(name_part);
            entries.push(TreeEntry::File { path: full_path });
        }
    }

    entries
}

/// Parse a single tree line and return (depth, name).
///
/// Examples:
/// - "├── packages/" -> (0, "packages/")
/// - "│   ├── alpha/" -> (1, "alpha/")
/// - "    └── beta/" -> (1, "beta/")
fn parse_tree_line(line: &str) -> (usize, &str) {
    let mut depth = 0;
    let mut chars = line.chars().peekable();
    let mut byte_pos = 0;

    loop {
        // Look for the start of a connector ("├──" or "└──")
        match chars.peek() {
            Some('├' | '└') => {
                // Found a connector, skip "├── " or "└── " (4 chars, varying bytes)
                // Skip the connector character
                let c = chars.next().unwrap();
                byte_pos += c.len_utf8();

                // Skip "── " (3 chars = 6 bytes for em-dashes + 1 for space = 7, but could be regular dashes)
                // Actually the dashes are "──" which could be regular ASCII or unicode
                // Let's just skip until we hit a non-dash, non-space character that's the name
                while let Some(&c) = chars.peek() {
                    if c == '─' || c == '-' || c == ' ' {
                        chars.next();
                        byte_pos += c.len_utf8();
                    } else {
                        break;
                    }
                }

                // Return the name part
                return (depth, &line[byte_pos..]);
            }
            Some('│') => {
                // Part of indentation, skip "│   " (4 positions)
                chars.next();
                byte_pos += '│'.len_utf8();
                // Skip following spaces (should be 3)
                for _ in 0..3 {
                    if let Some(&c) = chars.peek() {
                        if c == ' ' {
                            chars.next();
                            byte_pos += 1;
                        }
                    }
                }
                depth += 1;
            }
            Some(' ') => {
                // Part of indentation after a "└──" in parent, skip "    " (4 spaces)
                let mut spaces = 0;
                while let Some(&c) = chars.peek() {
                    if c == ' ' && spaces < 4 {
                        chars.next();
                        byte_pos += 1;
                        spaces += 1;
                    } else {
                        break;
                    }
                }
                if spaces == 4 {
                    depth += 1;
                }
            }
            _ => {
                // No more prefix, this is the name (or empty)
                return (depth, &line[byte_pos..]);
            }
        }
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

        // Extract files and commands from steps
        let files: Vec<_> = test
            .steps
            .iter()
            .filter_map(|s| match s {
                TestStep::WriteFile(f) => Some(f),
                _ => None,
            })
            .collect();
        let commands: Vec<_> = test
            .steps
            .iter()
            .filter_map(|s| match s {
                TestStep::RunCommand(c) => Some(c),
                _ => None,
            })
            .collect();

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, PathBuf::from("pyproject.toml"));
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].command, "uv lock");
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
            result.tests[0].config.environment.python_versions,
            crate::types::PythonVersions::Only(vec!["3.12".to_string()])
        );
        assert_eq!(
            result.tests[1].config.environment.python_versions,
            crate::types::PythonVersions::Only(vec!["3.12".to_string()])
        );
    }

    #[test]
    fn test_parse_with_file_level_create_venv() {
        let source = r#"
```toml title="mdtest.toml"
[environment]
python-version = "3.12"
create-venv = false
```

# Tests

## Test one

```
$ uv venv
success: true
exit_code: 0
----- stdout -----

----- stderr -----
Done
```
"#;

        let result = MarkdownTestFile::parse(PathBuf::from("test.md"), source).unwrap();
        assert_eq!(result.tests.len(), 1);

        // Test should have the file-level config including create_venv
        assert_eq!(
            result.tests[0].config.environment.python_versions,
            crate::types::PythonVersions::Only(vec!["3.12".to_string()])
        );
        assert_eq!(result.tests[0].config.environment.create_venv, Some(false));
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

        // Extract files from steps
        let files_a: Vec<_> = result.tests[0]
            .steps
            .iter()
            .filter_map(|s| match s {
                TestStep::WriteFile(f) => Some(f),
                _ => None,
            })
            .collect();
        let files_b: Vec<_> = result.tests[1]
            .steps
            .iter()
            .filter_map(|s| match s {
                TestStep::WriteFile(f) => Some(f),
                _ => None,
            })
            .collect();

        // Test A should only have a.toml
        assert_eq!(files_a.len(), 1);
        assert_eq!(files_a[0].path, PathBuf::from("a.toml"));

        // Test B should only have b.toml (no inheritance from A)
        assert_eq!(files_b.len(), 1);
        assert_eq!(files_b[0].path, PathBuf::from("b.toml"));
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
        let file_snapshots: Vec<_> = test
            .steps
            .iter()
            .filter_map(|s| match s {
                TestStep::CheckFileSnapshot(f) => Some(f),
                _ => None,
            })
            .collect();
        assert_eq!(file_snapshots.len(), 1);
        assert_eq!(file_snapshots[0].path, PathBuf::from("uv.lock"));
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
