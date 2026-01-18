//! Snapshot management for markdown tests.
//!
//! This module handles updating markdown files when snapshots need to be updated.

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use fs_err as fs;

use thiserror::Error;

use uv_static::EnvVars;

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

    #[error("Failed to create temp file: {0}")]
    TempFile(std::io::Error),
}

/// A pending snapshot update.
#[derive(Debug, Clone)]
struct PendingUpdate {
    /// Line number in the source file.
    line: usize,
    /// New content to write.
    content: String,
}

/// Collects snapshot updates and applies them atomically.
///
/// This handles concurrent test execution by collecting all mismatches
/// and applying them in a single atomic write per file.
#[derive(Debug, Default)]
pub struct SnapshotUpdater {
    /// Pending updates grouped by source file path.
    pending: Mutex<HashMap<PathBuf, Vec<PendingUpdate>>>,
}

impl SnapshotUpdater {
    /// Create a new snapshot updater.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a mismatch to be updated.
    ///
    /// This is thread-safe and can be called from multiple test threads.
    pub fn add(&self, source_path: &Path, mismatch: &Mismatch) {
        // Construct the full content for the code block
        let content = match &mismatch.kind {
            MismatchKind::CommandOutput { command } => {
                // Command blocks need to preserve the "$ command" line
                format!("$ {command}\n{}", mismatch.actual)
            }
            MismatchKind::FileSnapshot { .. } => {
                // File snapshots are just the content
                mismatch.actual.clone()
            }
        };

        let update = PendingUpdate {
            line: mismatch.line,
            content,
        };

        let mut pending = self.pending.lock().unwrap();
        pending
            .entry(source_path.to_path_buf())
            .or_default()
            .push(update);
    }

    /// Commit all pending updates.
    ///
    /// For each file, updates are applied from bottom to top to avoid
    /// line number shifts. Each file is written atomically using a
    /// tempfile and rename.
    pub fn commit(self) -> Result<Vec<PathBuf>, SnapshotError> {
        let pending = self.pending.into_inner().unwrap();
        let mut updated_files = Vec::new();

        for (path, mut updates) in pending {
            if updates.is_empty() {
                continue;
            }

            // Sort by line number descending so we update from bottom to top
            // This prevents earlier updates from shifting line numbers of later updates
            updates.sort_by(|a, b| b.line.cmp(&a.line));

            // Read the file
            let content = fs::read_to_string(&path).map_err(|e| SnapshotError::ReadFile {
                path: path.display().to_string(),
                source: e,
            })?;

            // Apply all updates
            let mut result = content;
            for update in updates {
                result = update_code_block_content(&result, update.line, &update.content)?;
            }

            // Write atomically using tempfile + rename
            let parent = path.parent().ok_or_else(|| SnapshotError::WriteFile {
                path: path.display().to_string(),
                source: std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "No parent directory",
                ),
            })?;

            let mut temp_file =
                tempfile::NamedTempFile::new_in(parent).map_err(SnapshotError::TempFile)?;

            temp_file
                .write_all(result.as_bytes())
                .map_err(|e| SnapshotError::WriteFile {
                    path: path.display().to_string(),
                    source: e,
                })?;

            temp_file.flush().map_err(|e| SnapshotError::WriteFile {
                path: path.display().to_string(),
                source: e,
            })?;

            temp_file
                .persist(&path)
                .map_err(|e| SnapshotError::WriteFile {
                    path: path.display().to_string(),
                    source: e.error,
                })?;

            updated_files.push(path);
        }

        Ok(updated_files)
    }
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
        if std::env::var(EnvVars::UV_UPDATE_SNAPSHOTS).is_ok()
            || std::env::var("INSTA_UPDATE").is_ok_and(|v| v == "1" || v == "always")
        {
            Self::Update
        } else {
            Self::Compare
        }
    }
}

impl Mismatch {
    /// Update the snapshot in a markdown file.
    ///
    /// This replaces the content of a code block at the specified line with new content.
    pub fn update_snapshot(&self, markdown_path: &Path) -> Result<(), SnapshotError> {
        let content = fs::read_to_string(markdown_path).map_err(|e| SnapshotError::ReadFile {
            path: markdown_path.display().to_string(),
            source: e,
        })?;

        let new_content = match &self.kind {
            MismatchKind::CommandOutput { .. } => {
                update_code_block_content(&content, self.line, &self.actual)?
            }
            MismatchKind::FileSnapshot { .. } => {
                update_code_block_content(&content, self.line, &self.actual)?
            }
        };

        fs::write(markdown_path, new_content).map_err(|e| SnapshotError::WriteFile {
            path: markdown_path.display().to_string(),
            source: e,
        })?;

        Ok(())
    }

    /// Format this mismatch for display.
    pub fn format(&self) -> String {
        use std::fmt::Write;

        let mut output = String::new();

        match &self.kind {
            MismatchKind::CommandOutput { command } => {
                let _ = writeln!(output, "Command output mismatch for `{command}`:");
            }
            MismatchKind::FileSnapshot { path } => {
                let _ = writeln!(output, "File snapshot mismatch for `{}`:", path.display());
            }
        }

        let _ = writeln!(output, "\n--- expected (line {})", self.line);
        let _ = writeln!(output, "+++ actual\n");

        // Generate a simple diff
        let expected_lines: Vec<&str> = self.expected.lines().collect();
        let actual_lines: Vec<&str> = self.actual.lines().collect();

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
    let mut found = false;

    while i < lines.len() {
        let line_num = i + 1; // 1-indexed

        // Check if this is the start of our target code block (only update the first match)
        if !found && line_num >= target_line && lines[i].starts_with("```") {
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

            // Mark that we've found and updated the target block
            found = true;
        } else {
            result.push(lines[i].to_string());
        }

        i += 1;
    }

    if !found {
        return Err(SnapshotError::CodeBlockNotFound { line: target_line });
    }

    Ok(result.join("\n"))
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

        let output = mismatch.format();
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
            std::env::remove_var(EnvVars::UV_UPDATE_SNAPSHOTS);
            std::env::remove_var("INSTA_UPDATE");
        }
        assert_eq!(SnapshotMode::from_env(), SnapshotMode::Compare);
    }

    #[test]
    fn test_snapshot_updater_single_update() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test.md");

        let content = r"# Test

```
$ uv version
old output
```
";
        fs::write(&file_path, content).unwrap();

        let updater = SnapshotUpdater::new();
        let mismatch = Mismatch {
            kind: MismatchKind::CommandOutput {
                command: "uv version".to_string(),
            },
            expected: "old output".to_string(),
            actual: "new output".to_string(),
            line: 3,
        };

        updater.add(&file_path, &mismatch);
        let updated_files = updater.commit().unwrap();

        assert_eq!(updated_files.len(), 1);
        assert_eq!(updated_files[0], file_path);

        let result = fs::read_to_string(&file_path).unwrap();
        assert!(
            result.contains("$ uv version"),
            "Command should be preserved"
        );
        assert!(
            result.contains("new output"),
            "New output should be present"
        );
        assert!(
            !result.contains("old output"),
            "Old output should be replaced"
        );
    }

    #[test]
    fn test_snapshot_updater_multiple_updates_same_file() {
        // Tests that multiple updates in the same file are applied correctly
        // Updates should be applied from bottom to top to avoid line number shifts
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test.md");

        let content = r"# Test

## First test

```
$ uv version
first old
```

## Second test

```
$ uv lock
second old
```
";
        fs::write(&file_path, content).unwrap();

        let updater = SnapshotUpdater::new();

        // Add first mismatch (line 5)
        let mismatch1 = Mismatch {
            kind: MismatchKind::CommandOutput {
                command: "uv version".to_string(),
            },
            expected: "first old".to_string(),
            actual: "first new".to_string(),
            line: 5,
        };

        // Add second mismatch (line 12)
        let mismatch2 = Mismatch {
            kind: MismatchKind::CommandOutput {
                command: "uv lock".to_string(),
            },
            expected: "second old".to_string(),
            actual: "second new".to_string(),
            line: 12,
        };

        updater.add(&file_path, &mismatch1);
        updater.add(&file_path, &mismatch2);
        let updated_files = updater.commit().unwrap();

        assert_eq!(updated_files.len(), 1);

        let result = fs::read_to_string(&file_path).unwrap();
        assert!(
            result.contains("$ uv version"),
            "First command should be preserved"
        );
        assert!(
            result.contains("$ uv lock"),
            "Second command should be preserved"
        );
        assert!(
            result.contains("first new"),
            "First update should be applied"
        );
        assert!(
            result.contains("second new"),
            "Second update should be applied"
        );
        assert!(
            !result.contains("first old"),
            "First old content should be replaced"
        );
        assert!(
            !result.contains("second old"),
            "Second old content should be replaced"
        );
    }

    #[test]
    fn test_snapshot_updater_multiple_files() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file1 = temp_dir.path().join("test1.md");
        let file2 = temp_dir.path().join("test2.md");

        fs::write(
            &file1,
            r"# Test 1

```
$ uv version
old1
```
",
        )
        .unwrap();

        fs::write(
            &file2,
            r"# Test 2

```
$ uv lock
old2
```
",
        )
        .unwrap();

        let updater = SnapshotUpdater::new();

        updater.add(
            &file1,
            &Mismatch {
                kind: MismatchKind::CommandOutput {
                    command: "uv version".to_string(),
                },
                expected: "old1".to_string(),
                actual: "new1".to_string(),
                line: 3,
            },
        );

        updater.add(
            &file2,
            &Mismatch {
                kind: MismatchKind::CommandOutput {
                    command: "uv lock".to_string(),
                },
                expected: "old2".to_string(),
                actual: "new2".to_string(),
                line: 3,
            },
        );

        let updated_files = updater.commit().unwrap();
        assert_eq!(updated_files.len(), 2);

        let result1 = fs::read_to_string(&file1).unwrap();
        let result2 = fs::read_to_string(&file2).unwrap();

        assert!(result1.contains("new1"));
        assert!(result2.contains("new2"));
    }

    #[test]
    fn test_snapshot_updater_preserves_surrounding_content() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test.md");

        let content = r"# My Test Document

Some introductory text here.

## Test Section

```
$ uv version
old output
```

Some text after the code block.

## Another Section

More content here that should be preserved.
";
        fs::write(&file_path, content).unwrap();

        let updater = SnapshotUpdater::new();
        updater.add(
            &file_path,
            &Mismatch {
                kind: MismatchKind::CommandOutput {
                    command: "uv version".to_string(),
                },
                expected: "old output".to_string(),
                actual: "new output".to_string(),
                line: 7,
            },
        );

        updater.commit().unwrap();

        let result = fs::read_to_string(&file_path).unwrap();

        // Check that surrounding content is preserved
        assert!(result.contains("# My Test Document"));
        assert!(result.contains("Some introductory text here."));
        assert!(result.contains("## Test Section"));
        assert!(result.contains("Some text after the code block."));
        assert!(result.contains("## Another Section"));
        assert!(result.contains("More content here that should be preserved."));

        // Check that the update was applied
        assert!(result.contains("new output"));
        assert!(!result.contains("old output"));
    }

    #[test]
    fn test_snapshot_updater_line_count_change() {
        // Test that updates work correctly when the new content has a different
        // number of lines than the old content
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test.md");

        let content = r"# Test

## First

```
$ cmd1
one line
```

## Second

```
$ cmd2
original
```
";
        fs::write(&file_path, content).unwrap();

        let updater = SnapshotUpdater::new();

        // First update: expand from 1 line to 3 lines
        updater.add(
            &file_path,
            &Mismatch {
                kind: MismatchKind::CommandOutput {
                    command: "cmd1".to_string(),
                },
                expected: "one line".to_string(),
                actual: "line 1\nline 2\nline 3".to_string(),
                line: 5,
            },
        );

        // Second update: at original line 12, should still work after first update shifts lines
        updater.add(
            &file_path,
            &Mismatch {
                kind: MismatchKind::CommandOutput {
                    command: "cmd2".to_string(),
                },
                expected: "original".to_string(),
                actual: "updated".to_string(),
                line: 12,
            },
        );

        updater.commit().unwrap();

        let result = fs::read_to_string(&file_path).unwrap();

        // Commands should be preserved
        assert!(
            result.contains("$ cmd1"),
            "First command should be preserved"
        );
        assert!(
            result.contains("$ cmd2"),
            "Second command should be preserved"
        );
        // New content should be present
        assert!(
            result.contains("line 1\nline 2\nline 3"),
            "First update should expand to 3 lines"
        );
        assert!(
            result.contains("updated"),
            "Second update should be applied"
        );
        // Old content should be gone
        assert!(
            !result.contains("one line"),
            "First old content should be replaced"
        );
        assert!(
            !result.contains("original"),
            "Second old content should be replaced"
        );
    }

    #[test]
    fn test_snapshot_updater_empty() {
        // Test that committing with no updates works
        let updater = SnapshotUpdater::new();
        let updated_files = updater.commit().unwrap();
        assert!(updated_files.is_empty());
    }

    #[test]
    fn test_snapshot_updater_thread_safety() {
        // Test that the updater can be used from multiple threads
        use std::sync::Arc;
        use std::thread;

        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test.md");

        // Create a file with many test sections
        // Structure for each section (7 lines):
        // ## Section N\n  (1 line)
        // \n             (1 line)
        // ```\n          (1 line) <- this is what we target
        // $ cmdN\n       (1 line)
        // oldN\n         (1 line)
        // ```\n          (1 line)
        // \n             (1 line)
        let mut content = String::from("# Test\n\n");
        for i in 0..10 {
            content.push_str(&format!("## Section {i}\n\n```\n$ cmd{i}\nold{i}\n```\n\n"));
        }
        fs::write(&file_path, &content).unwrap();

        let updater = Arc::new(SnapshotUpdater::new());
        let mut handles = vec![];

        // Spawn threads that each add a mismatch
        for i in 0..10 {
            let updater = Arc::clone(&updater);
            let file_path = file_path.clone();
            let handle = thread::spawn(move || {
                let mismatch = Mismatch {
                    kind: MismatchKind::CommandOutput {
                        command: format!("cmd{i}"),
                    },
                    expected: format!("old{i}"),
                    actual: format!("new{i}"),
                    // Line numbers: header is 2 lines, then each section is 7 lines
                    // Section 0: starts at line 3 (header) + 2 = 5 for ```
                    // Section 1: starts at line 3 + 7 = 10 for ```, so 5 + 7 = 12
                    // Pattern: 5 + i * 7
                    line: 5 + i * 7,
                };
                updater.add(&file_path, &mismatch);
            });
            handles.push(handle);
        }

        // Wait for all threads
        for handle in handles {
            handle.join().unwrap();
        }

        // Commit all updates
        let updater = Arc::try_unwrap(updater).unwrap();
        let updated_files = updater.commit().unwrap();

        assert_eq!(updated_files.len(), 1);

        let result = fs::read_to_string(&file_path).unwrap();

        // Verify all updates were applied
        for i in 0..10 {
            assert!(
                result.contains(&format!("$ cmd{i}")),
                "Command {i} should be preserved"
            );
            assert!(
                result.contains(&format!("new{i}")),
                "Update {i} should be applied"
            );
            assert!(
                !result.contains(&format!("old{i}")),
                "Old content {i} should be replaced"
            );
        }
    }
}
