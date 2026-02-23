//! Unix-specific functionality for uv.
//!
//! This crate is only functional on Unix platforms.

#![cfg(unix)]

mod ctrl_c;
mod resource_limits;

pub use ctrl_c::{CtrlCError, on_ctrl_c};
pub use resource_limits::{OpenFileLimitError, adjust_open_file_limit};
