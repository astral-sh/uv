//! Windows-specific utilities for uv.
//!
//! This crate provides shared Windows functionality used by both the main `uv` crate
//! and `uv-trampoline`. It supports `no_std`-friendly usage via the `std` feature flag.

#![cfg(windows)]

mod ctrl_c;
mod job;
#[cfg(feature = "std")]
mod spawn;

#[cfg(feature = "std")]
pub use ctrl_c::on_ctrl_c;
pub use ctrl_c::{CtrlCError, ignore_ctrl_c};
pub use job::{Job, JobError};
#[cfg(feature = "std")]
pub use spawn::spawn_child;
