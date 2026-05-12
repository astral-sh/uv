//! Unix-specific functionality for uv.
//!
//! This crate is only functional on Unix platforms.

#![cfg(unix)]

#[cfg(target_os = "macos")]
pub mod macho;

mod resource_limits;

pub use resource_limits::{OpenFileLimitError, adjust_open_file_limit};
