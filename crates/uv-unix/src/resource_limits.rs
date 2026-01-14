//! Helper for adjusting Unix resource limits.
//!
//! Linux has a historically low default limit of 1024 open file descriptors per process.
//! macOS also defaults to a low soft limit (typically 256), though its hard limit is much
//! higher. On modern multi-core machines, these low defaults can cause "too many open files"
//! errors because uv infers concurrency limits from CPU count and may schedule more concurrent
//! work than the default file descriptor limit allows.
//!
//! This module attempts to raise the soft limit to the hard limit at startup to avoid these
//! errors without requiring users to manually configure their shell's `ulimit` settings.
//! The raised limit is inherited by child processes, which is important for commands like
//! `uv run` that spawn Python interpreters.
//!
//! See: <https://github.com/astral-sh/uv/issues/16999>

use nix::sys::resource::{Resource, getrlimit, setrlimit};
use tracing::debug;

/// Maximum file descriptor limit to request.
///
/// We cap at 0x100000 (1,048,576) to match the typical Linux default (`/proc/sys/fs/nr_open`)
/// and to avoid issues with extremely high limits.
///
/// `OpenJDK` uses this same cap because:
///
/// 1. Some code breaks if `RLIMIT_NOFILE` exceeds `i32::MAX` (despite the type being `u64`)
/// 2. Code that iterates over all possible FDs to close them can timeout with very high limits
///
/// See: <https://bugs.openjdk.org/browse/JDK-8324577>
/// See: <https://github.com/oracle/graal/issues/11136>
const MAX_NOFILE_LIMIT: u64 = 0x0010_0000;

/// Attempt to raise the open file descriptor limit to the maximum allowed.
///
/// This function tries to set the soft limit to `min(hard_limit, 0x100000)`. If the operation
/// fails, it silently ignores the error since the default limits may still be sufficient for
/// the current workload.
///
/// Returns the new soft limit on success, or the current soft limit if adjustment failed.
pub fn adjust_open_file_limit() -> u64 {
    let (soft, hard) = match getrlimit(Resource::RLIMIT_NOFILE) {
        Ok(limits) => limits,
        Err(err) => {
            debug!("Failed to get open file limit: {err}");
            return 0;
        }
    };

    debug!("Current open file limits: soft={soft}, hard={hard}");

    // Cap the target limit to avoid issues with extremely high values
    let target = hard.min(MAX_NOFILE_LIMIT);

    if soft >= target {
        return soft;
    }

    // Try to raise the soft limit to the target
    match setrlimit(Resource::RLIMIT_NOFILE, target, hard) {
        Ok(()) => {
            debug!("Raised open file limit from {soft} to {target}");
            target
        }
        Err(err) => {
            debug!("Failed to raise open file limit from {soft} to {target}: {err}");
            soft
        }
    }
}
