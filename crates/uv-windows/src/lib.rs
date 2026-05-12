//! Windows-specific utilities for uv.
//!
//! This crate provides shared Windows functionality used by both the main `uv` crate
//! and `uv-trampoline`. It supports `no_std`-friendly usage via the `std` feature flag.

#![cfg(windows)]

mod ctrl_handler;
mod job;
#[cfg(feature = "std")]
mod spawn;

pub use ctrl_handler::{CtrlHandlerError, install_ctrl_handler};
pub use job::{Job, JobError};
#[cfg(feature = "std")]
pub use spawn::spawn_child;
