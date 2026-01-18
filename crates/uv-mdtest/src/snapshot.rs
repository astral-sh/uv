//! Snapshot management for markdown tests.
//!
//! This module handles updating markdown files when snapshots need to be updated.

use std::path::Path;

use fs_err as fs;

use thiserror::Error;

use crate::runner::{Mismatch, MismatchKind};

/// Errors that can occur during snapshot operations.
#[derive(Debug, Error)]
pub enum SnapshotError {
    #[error("Failed to read file {path}: {source}")]
    ReadFile {
        path: String,
        source: std::io::Error,
    },

    #[error("Failed to write file {path}: {source}")]
    WriteFile {
        path: String,
        source: std::io::Error,
    },

    #[error("Could not find code block at line {line}")]
    CodeBlockNotFound { line: usize },
}

/// Mode for snapshot handling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotMode {
    /// Compare snapshots and fail on mismatch.
    Compare,
    /// Update snapshots in place.
    Update,
}

impl SnapshotMode {
    /// Get the mode from environment variable.
    pub fn from_env() -> Self {
        // Check for UV_UPDATE_SNAPSHOTS or INSTA_UPDATE
        if std::env::var("UV_UPDATE_SNAPSHOTS").is_ok()
            || std::env::var("INSTA_UPDATE").is_ok_and(|v| v == "1" || v == "always")
        {
            Self::Update
        } else {
            Self::Compare
        }
    }
}

/// Update a snapshot in a markdown file.
///
/// This replaces the content of a code block at the specified line with new content.
pub fn update_snapshot(markdown_path: &Path, mismatch: &Mismatch) -> Result<(), SnapshotError> {
    let content = fs::read_to_string(markdown_path).map_err(|e| SnapshotError::ReadFile {
        path: markdown_path.display().to_string(),
        source: e,
    })?;

    let new_content = match &mismatch.kind {
        MismatchKind::CommandOutput { .. } => {
            update_code_block_content(&content, mismatch.line, &mismatch.actual)?
        }
        MismatchKind::FileSnapshot { .. } => {
            update_code_block_content(&content, mismatch.line, &mismatch.actual)?
        }
    };

    fs::write(markdown_path, new_content).map_err(|e| SnapshotError::WriteFile {
        path: markdown_path.display().to_string(),
        source: e,
    })?;

    Ok(())
}

/// Update the content of a code block at a specific line.
fn update_code_block_content(
    source: &str,
    target_line: usize,
    new_content: &str,
) -> Result<String, SnapshotError> {
    let lines: Vec<&str> = source.lines().collect();
    let mut result = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        let line_num = i + 1; // 1-indexed

        // Check if this is the start of our target code block
        if line_num >= target_line && lines[i].starts_with("```") {
            // Find the end of this code block
            let start_line = i;
            i += 1;

            // Skip to the closing ```
            while i < lines.len() && !lines[i].starts_with("```") {
                i += 1;
            }

            if i >= lines.len() {
                return Err(SnapshotError::CodeBlockNotFound { line: target_line });
            }

            // Add the opening ``` line
            result.push(lines[start_line].to_string());

            // Add the new content
            for line in new_content.lines() {
                result.push(line.to_string());
            }

            // Add the closing ``` line
            result.push(lines[i].to_string());
        } else {
            result.push(lines[i].to_string());
        }

        i += 1;
    }

    Ok(result.join("\n"))
}

/// Format a mismatch for display.
pub fn format_mismatch(mismatch: &Mismatch) -> String {
    use std::fmt::Write;

    let mut output = String::new();

    match &mismatch.kind {
        MismatchKind::CommandOutput { command } => {
            let _ = writeln!(output, "Command output mismatch for `{command}`:");
        }
        MismatchKind::FileSnapshot { path } => {
            let _ = writeln!(output, "File snapshot mismatch for `{}`:", path.display());
        }
    }

    let _ = writeln!(output, "\n--- expected (line {})", mismatch.line);
    let _ = writeln!(output, "+++ actual\n");

    // Generate a simple diff
    let expected_lines: Vec<&str> = mismatch.expected.lines().collect();
    let actual_lines: Vec<&str> = mismatch.actual.lines().collect();

    let max_len = expected_lines.len().max(actual_lines.len());
    for i in 0..max_len {
        let exp = expected_lines.get(i).copied().unwrap_or("");
        let act = actual_lines.get(i).copied().unwrap_or("");

        if exp != act {
            if !exp.is_empty() {
                let _ = writeln!(output, "-{exp}");
            }
            if !act.is_empty() {
                let _ = writeln!(output, "+{act}");
            }
        } else {
            let _ = writeln!(output, " {exp}");
        }
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_update_code_block_content() {
        let source = r"# Test

```
$ uv lock
old content
```

More text.
";

        let result = update_code_block_content(source, 3, "$ uv lock\nnew content").unwrap();

        assert!(result.contains("new content"));
        assert!(!result.contains("old content"));
        assert!(result.contains("More text"));
    }

    #[test]
    fn test_format_mismatch() {
        let mismatch = Mismatch {
            kind: MismatchKind::CommandOutput {
                command: "uv lock".to_string(),
            },
            expected: "success: true\nexit_code: 0".to_string(),
            actual: "success: false\nexit_code: 1".to_string(),
            line: 10,
        };

        let output = format_mismatch(&mismatch);
        assert!(output.contains("Command output mismatch"));
        assert!(output.contains("-success: true"));
        assert!(output.contains("+success: false"));
    }

    #[test]
    #[expect(unsafe_code)]
    fn test_snapshot_mode_from_env() {
        // Default should be Compare
        // SAFETY: This is a single-threaded test and we're only modifying
        // test-specific environment variables.
        unsafe {
            std::env::remove_var("UV_UPDATE_SNAPSHOTS");
            std::env::remove_var("INSTA_UPDATE");
        }
        assert_eq!(SnapshotMode::from_env(), SnapshotMode::Compare);
    }
}
