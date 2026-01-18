//! Markdown-based test framework for uv.
//!
//! This crate provides a way to write tests for uv commands in markdown format,
//! inspired by [ty's mdtest framework](https://github.com/astral-sh/ruff/tree/main/crates/ty_test).
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

pub use parser::ParseError;
pub use runner::{Mismatch, MismatchKind, RunConfig, RunError, TestResult};
pub use snapshot::{SnapshotMode, SnapshotUpdater};
pub use types::{
    EnvironmentConfig, FilterConfig, MarkdownTest, MarkdownTestFile, TargetFamily, TargetOs,
    TestConfig,
};
