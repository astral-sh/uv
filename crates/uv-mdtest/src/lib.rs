//! Markdown-based test framework for uv.
//!
//! This crate provides a way to write tests for uv commands in markdown format,
//! inspired by ty's mdtest framework.
//!
//! # Format Overview
//!
//! Tests are written in markdown files with the following structure:
//!
//! - **Headers** organize tests into a hierarchy. Each leaf section (containing code blocks) becomes a test.
//! - **Code blocks with `title="filename"`** create files in the test directory.
//! - **Code blocks starting with `$ `** are commands to execute, with expected output following.
//! - **Code blocks with `snapshot=true`** are files to snapshot after commands run.
//! - **Code blocks with `title="mdtest.toml"`** configure the test.
//!
//! # Example
//!
//! ````markdown
//! # Lock
//!
//! Tests for `uv lock` command.
//!
//! ## Basic locking
//!
//! ```toml title="pyproject.toml"
//! [project]
//! name = "test"
//! version = "0.1.0"
//! dependencies = ["iniconfig"]
//! ```
//!
//! ```
//! $ uv lock
//! success: true
//! exit_code: 0
//! ----- stdout -----
//!
//! ----- stderr -----
//! Resolved 2 packages in [TIME]
//! ```
//! ````
//!
//!
//! # Configuration
//!
//! Use `mdtest.toml` blocks to configure tests:
//!
//! ````markdown
//! ```toml title="mdtest.toml"
//! [environment]
//! python-version = "3.12"
//! exclude-newer = "2024-03-25T00:00:00Z"
//! ```
//! ````

pub mod parser;
pub mod runner;
pub mod snapshot;
pub mod types;

pub use parser::{ParseError, parse};
pub use runner::{RunConfig, RunError, TestResult, run_test, run_test_with_command_builder};
pub use snapshot::{SnapshotMode, format_mismatch, update_snapshot};
pub use types::{EnvironmentConfig, FilterConfig, MarkdownTest, MarkdownTestFile, TestConfig};
