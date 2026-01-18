//! Test runner for markdown tests.
//!
//! This module provides the logic for executing markdown tests. The actual
//! integration with the test framework happens in the test entry point.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use fs_err as fs;
use std::process::{Command, Output};

use regex::Regex;
use thiserror::Error;

use crate::types::{MarkdownTest, TestConfig};

/// Errors that can occur during test execution.
#[derive(Debug, Error)]
pub enum RunError {
    #[error("Failed to create directory {path}: {source}")]
    CreateDir {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("Failed to write file {path}: {source}")]
    WriteFile {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("Failed to read file {path}: {source}")]
    ReadFile {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("Failed to execute command: {source}")]
    ExecuteCommand { source: std::io::Error },

    #[error("Command output mismatch at line {line}")]
    OutputMismatch { line: usize },

    #[error("File snapshot mismatch for {path} at line {line}")]
    SnapshotMismatch { path: PathBuf, line: usize },
}

/// Result of running a single test.
#[derive(Debug)]
pub struct TestResult {
    /// Name of the test.
    pub name: String,
    /// Whether the test passed.
    pub passed: bool,
    /// Any mismatch details if the test failed.
    pub mismatch: Option<Mismatch>,
}

/// Details about a test mismatch.
#[derive(Debug)]
pub struct Mismatch {
    /// Kind of mismatch.
    pub kind: MismatchKind,
    /// Expected content.
    pub expected: String,
    /// Actual content.
    pub actual: String,
    /// Line number in the markdown source.
    pub line: usize,
}

/// Kind of mismatch.
#[derive(Debug)]
pub enum MismatchKind {
    /// Command output didn't match.
    CommandOutput { command: String },
    /// File snapshot didn't match.
    FileSnapshot { path: PathBuf },
}

/// Configuration for running tests.
pub struct RunConfig {
    /// Base directory for test execution.
    pub temp_dir: PathBuf,
    /// Path to the uv binary.
    pub uv_binary: PathBuf,
    /// Cache directory.
    pub cache_dir: PathBuf,
    /// Output filters (regex patterns).
    pub filters: Vec<(Regex, String)>,
    /// Extra environment variables.
    pub env: HashMap<String, String>,
}

impl RunConfig {
    /// Apply filters to output.
    pub fn apply_filters(&self, mut output: String) -> String {
        for (regex, replacement) in &self.filters {
            output = regex.replace_all(&output, replacement.as_str()).to_string();
        }
        output
    }
}

/// Run a single markdown test.
pub fn run_test(test: &MarkdownTest, config: &RunConfig) -> Result<TestResult, RunError> {
    // Create the test directory
    let test_dir = &config.temp_dir;
    fs::create_dir_all(test_dir).map_err(|e| RunError::CreateDir {
        path: test_dir.clone(),
        source: e,
    })?;

    // Write embedded files
    for file in &test.files {
        let file_path = test_dir.join(&file.path);
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent).map_err(|e| RunError::CreateDir {
                path: parent.to_path_buf(),
                source: e,
            })?;
        }
        fs::write(&file_path, &file.content).map_err(|e| RunError::WriteFile {
            path: file_path,
            source: e,
        })?;
    }

    // Execute commands
    for cmd in &test.commands {
        let result = run_command(&cmd.command, config, test_dir, &test.config)?;
        let filtered_output = config.apply_filters(result);

        if filtered_output.trim() != cmd.expected_output.trim() {
            return Ok(TestResult {
                name: test.name.clone(),
                passed: false,
                mismatch: Some(Mismatch {
                    kind: MismatchKind::CommandOutput {
                        command: cmd.command.clone(),
                    },
                    expected: cmd.expected_output.clone(),
                    actual: filtered_output,
                    line: cmd.line_number,
                }),
            });
        }
    }

    // Check file snapshots
    for snapshot in &test.file_snapshots {
        let file_path = test_dir.join(&snapshot.path);
        let actual_content = fs::read_to_string(&file_path).map_err(|e| RunError::ReadFile {
            path: file_path.clone(),
            source: e,
        })?;
        let filtered_content = config.apply_filters(actual_content);

        if filtered_content.trim() != snapshot.expected_content.trim() {
            return Ok(TestResult {
                name: test.name.clone(),
                passed: false,
                mismatch: Some(Mismatch {
                    kind: MismatchKind::FileSnapshot {
                        path: snapshot.path.clone(),
                    },
                    expected: snapshot.expected_content.clone(),
                    actual: filtered_content,
                    line: snapshot.line_number,
                }),
            });
        }
    }

    Ok(TestResult {
        name: test.name.clone(),
        passed: true,
        mismatch: None,
    })
}

/// Apply filters to output.
fn apply_filters(filters: &[(Regex, String)], mut output: String) -> String {
    for (regex, replacement) in filters {
        output = regex.replace_all(&output, replacement.as_str()).to_string();
    }
    output
}

/// Run a test using a custom command builder.
///
/// This allows integration with external test frameworks like `TestContext`.
pub fn run_test_with_command_builder<F>(
    test: &MarkdownTest,
    test_dir: &Path,
    filters: &[(Regex, String)],
    command_builder: F,
) -> Result<TestResult, RunError>
where
    F: Fn(&str) -> Command,
{
    // Execute commands
    for cmd in &test.commands {
        let result = run_command_with_builder(&cmd.command, test_dir, &command_builder)?;
        let filtered_output = apply_filters(filters, result);

        if filtered_output.trim() != cmd.expected_output.trim() {
            return Ok(TestResult {
                name: test.name.clone(),
                passed: false,
                mismatch: Some(Mismatch {
                    kind: MismatchKind::CommandOutput {
                        command: cmd.command.clone(),
                    },
                    expected: cmd.expected_output.clone(),
                    actual: filtered_output,
                    line: cmd.line_number,
                }),
            });
        }
    }

    // Check file snapshots
    for snapshot in &test.file_snapshots {
        let file_path = test_dir.join(&snapshot.path);
        let actual_content = fs::read_to_string(&file_path).map_err(|e| RunError::ReadFile {
            path: file_path.clone(),
            source: e,
        })?;
        let filtered_content = apply_filters(filters, actual_content);

        if filtered_content.trim() != snapshot.expected_content.trim() {
            return Ok(TestResult {
                name: test.name.clone(),
                passed: false,
                mismatch: Some(Mismatch {
                    kind: MismatchKind::FileSnapshot {
                        path: snapshot.path.clone(),
                    },
                    expected: snapshot.expected_content.clone(),
                    actual: filtered_content,
                    line: snapshot.line_number,
                }),
            });
        }
    }

    Ok(TestResult {
        name: test.name.clone(),
        passed: true,
        mismatch: None,
    })
}

/// Run a command using a command builder and return the formatted output.
fn run_command_with_builder<F>(
    command_line: &str,
    working_dir: &Path,
    command_builder: &F,
) -> Result<String, RunError>
where
    F: Fn(&str) -> Command,
{
    let mut cmd = command_builder(command_line);
    cmd.current_dir(working_dir);

    let output = cmd
        .output()
        .map_err(|e| RunError::ExecuteCommand { source: e })?;

    Ok(format_output(&output))
}

/// Run a uv command and return the formatted output.
fn run_command(
    command_line: &str,
    config: &RunConfig,
    working_dir: &Path,
    test_config: &TestConfig,
) -> Result<String, RunError> {
    // Parse the command line
    let parts: Vec<&str> = command_line.split_whitespace().collect();
    if parts.is_empty() {
        return Err(RunError::ExecuteCommand {
            source: std::io::Error::new(std::io::ErrorKind::InvalidInput, "Empty command"),
        });
    }

    let program = parts[0];
    let args = &parts[1..];

    // Build the command
    let mut cmd = if program == "uv" {
        let mut c = Command::new(&config.uv_binary);
        c.arg("--cache-dir").arg(&config.cache_dir);
        c.args(args);
        c
    } else {
        let mut c = Command::new(program);
        c.args(args);
        c
    };

    // Set working directory
    cmd.current_dir(working_dir);

    // Set environment variables
    cmd.env_clear();

    // Set basic environment
    cmd.env("HOME", working_dir);
    cmd.env("UV_NO_WRAP", "1");
    cmd.env("COLUMNS", "100");

    // Set exclude-newer if configured
    if let Some(exclude_newer) = &test_config.environment.exclude_newer {
        cmd.env("UV_EXCLUDE_NEWER", exclude_newer);
    }

    // Set additional env from config
    for (key, value) in &config.env {
        cmd.env(key, value);
    }

    // Set env from test config
    for (key, value) in &test_config.environment.env {
        cmd.env(key, value);
    }

    // Preserve PATH
    if let Ok(path) = std::env::var("PATH") {
        cmd.env("PATH", path);
    }

    // Execute and capture output
    let output = cmd
        .output()
        .map_err(|e| RunError::ExecuteCommand { source: e })?;

    Ok(format_output(&output))
}

/// Format command output in the `uv_snapshot` format.
fn format_output(output: &Output) -> String {
    let success = output.status.success();
    let exit_code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    format!(
        "success: {success}\n\
         exit_code: {exit_code}\n\
         ----- stdout -----\n\
         {stdout}\n\
         ----- stderr -----\n\
         {stderr}"
    )
    .trim_end()
    .to_string()
}

/// Compare two strings and generate a diff if they differ.
pub fn diff_strings(expected: &str, actual: &str) -> Option<String> {
    use std::fmt::Write;

    if expected.trim() == actual.trim() {
        return None;
    }

    let expected_lines: Vec<&str> = expected.lines().collect();
    let actual_lines: Vec<&str> = actual.lines().collect();

    let mut diff = String::new();
    diff.push_str("--- expected\n");
    diff.push_str("+++ actual\n");

    let max_len = expected_lines.len().max(actual_lines.len());
    for i in 0..max_len {
        let exp_line = expected_lines.get(i).copied().unwrap_or("");
        let act_line = actual_lines.get(i).copied().unwrap_or("");

        if exp_line != act_line {
            let _ = writeln!(diff, "-{exp_line}");
            let _ = writeln!(diff, "+{act_line}");
        } else {
            let _ = writeln!(diff, " {exp_line}");
        }
    }

    Some(diff)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_output() {
        let output = Output {
            status: std::process::ExitStatus::default(),
            stdout: b"hello\n".to_vec(),
            stderr: b"world\n".to_vec(),
        };

        let formatted = format_output(&output);
        assert!(formatted.contains("success: "));
        assert!(formatted.contains("exit_code: "));
        assert!(formatted.contains("----- stdout -----"));
        assert!(formatted.contains("----- stderr -----"));
        assert!(formatted.contains("hello"));
        assert!(formatted.contains("world"));
    }

    #[test]
    fn test_diff_strings_same() {
        assert!(diff_strings("hello\nworld", "hello\nworld").is_none());
    }

    #[test]
    fn test_diff_strings_different() {
        let diff = diff_strings("hello\nworld", "hello\nplanet").unwrap();
        assert!(diff.contains("-world"));
        assert!(diff.contains("+planet"));
    }

    #[test]
    fn test_apply_filters() {
        let config = RunConfig {
            temp_dir: PathBuf::from("/tmp"),
            uv_binary: PathBuf::from("uv"),
            cache_dir: PathBuf::from("/cache"),
            filters: vec![
                (Regex::new(r"\d+ms").unwrap(), "[TIME]".to_string()),
                (Regex::new(r"\d+\.\d+s").unwrap(), "[TIME]".to_string()),
            ],
            env: HashMap::new(),
        };

        let output = "Resolved in 123ms";
        assert_eq!(
            config.apply_filters(output.to_string()),
            "Resolved in [TIME]"
        );

        let output = "Resolved in 1.5s";
        assert_eq!(
            config.apply_filters(output.to_string()),
            "Resolved in [TIME]"
        );
    }
}
