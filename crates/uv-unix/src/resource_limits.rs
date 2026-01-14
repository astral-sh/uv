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

use nix::sys::resource::{Resource, getrlimit, rlim_t, setrlimit};
use tracing::debug;

/// Maximum file descriptor limit to request.
///
/// We cap at 0x100000 (1,048,576) to match the typical Linux default (`/proc/sys/fs/nr_open`)
/// and to avoid issues with extremely high limits.
///
/// `OpenJDK` uses this same cap because:
///
/// 1. Some code breaks if `RLIMIT_NOFILE` exceeds `i32::MAX` (despite the type being `u64`)
/// 2. Code that iterates over all possible FDs, e.g., to close them, can timeout
///
/// See: <https://bugs.openjdk.org/browse/JDK-8324577>
/// See: <https://github.com/oracle/graal/issues/11136>
///
/// Note: `rlim_t` is platform-specific (`u64` on Linux/macOS, `i64` on FreeBSD).
const MAX_NOFILE_LIMIT: rlim_t = 0x0010_0000;

/// Attempt to raise the open file descriptor limit to the maximum allowed.
///
/// This function tries to set the soft limit to `min(hard_limit, 0x100000)`. If the operation
/// fails, it silently ignores the error since the default limits may still be sufficient for
/// the current workload.
///
/// Returns the new soft limit on success, or the current soft limit if adjustment failed.
/// The return type is `u64` for API consistency across platforms, though `rlim_t` is
/// platform-specific (`u64` on Linux/macOS, `i64` on FreeBSD).
pub fn adjust_open_file_limit() -> u64 {
    let (soft, hard) = match getrlimit(Resource::RLIMIT_NOFILE) {
        Ok(limits) => limits,
        Err(err) => {
            debug!("Failed to get open file limit: {err}");
            return 0;
        }
    };

    debug!("Current open file limits: soft={soft}, hard={hard}");

    // Convert rlim_t to u64. On FreeBSD, rlim_t is i64, so we need try_from.
    // On Linux/macOS, rlim_t is u64, so the conversion is infallible.
    let Some(soft) = rlim_t_to_u64(soft) else {
        debug!("Soft limit is negative: {soft}");
        return 0;
    };

    // Cap the target limit to avoid issues with extremely high values.
    // If hard is negative or exceeds MAX_NOFILE_LIMIT, use MAX_NOFILE_LIMIT.
    #[allow(clippy::unnecessary_cast)]
    let target = rlim_t_to_u64(hard.min(MAX_NOFILE_LIMIT)).unwrap_or(MAX_NOFILE_LIMIT as u64);

    if soft >= target {
        return soft;
    }

    // Try to raise the soft limit to the target.
    // Safe because target <= MAX_NOFILE_LIMIT which fits in both i64 and u64.
    let target_rlim = target as rlim_t;

    match setrlimit(Resource::RLIMIT_NOFILE, target_rlim, hard) {
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

/// Convert `rlim_t` to `u64`, returning `None` if negative.
///
/// On Linux/macOS, `rlim_t` is `u64` so this always succeeds.
/// On FreeBSD, `rlim_t` is `i64` so negative values return `None`.
#[allow(clippy::unnecessary_cast, clippy::useless_conversion)]
fn rlim_t_to_u64(value: rlim_t) -> Option<u64> {
    u64::try_from(value).ok()
}
