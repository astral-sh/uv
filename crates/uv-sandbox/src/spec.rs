//! Resolved sandbox specification with concrete paths.
//!
//! This is the intermediate representation between the user-facing `SandboxOptions`
//! (which may contain presets and relative paths) and the OS-specific sandbox
//! implementation (which needs absolute paths).

use std::path::PathBuf;

/// A fully resolved sandbox specification.
///
/// All presets have been expanded, all paths are absolute, and all environment
/// variable filtering decisions are concrete.
#[derive(Debug, Clone)]
pub struct SandboxSpec {
    /// Paths the sandboxed process can read.
    pub allow_read: Vec<PathBuf>,
    /// Paths to deny reading (overrides allow).
    pub deny_read: Vec<PathBuf>,

    /// Paths the sandboxed process can write.
    pub allow_write: Vec<PathBuf>,
    /// Paths to deny writing (overrides allow).
    pub deny_write: Vec<PathBuf>,

    /// Paths the sandboxed process can execute.
    pub allow_execute: Vec<PathBuf>,
    /// Paths to deny executing (overrides allow).
    pub deny_execute: Vec<PathBuf>,

    /// Whether network access is allowed.
    pub allow_net: bool,

    /// Environment variables to pass through.
    ///
    /// `None` means pass all environment variables.
    /// `Some(vec)` means pass only the listed key-value pairs.
    pub env: Option<Vec<(String, String)>>,
}
