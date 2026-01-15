//! Unix-specific functionality for uv.
//!
//! This crate is only functional on Unix platforms.

#![cfg(unix)]

mod resource_limits;

pub use resource_limits::{OpenFileLimitError, adjust_open_file_limit};
