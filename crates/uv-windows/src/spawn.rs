//! Windows-specific process spawning with Job Objects.
//!
//! This module provides process spawning functionality that ensures child processes
//! are properly terminated when the parent process is killed. This is important for
//! tools like Task Scheduler that terminate wrapper processes.
//!
//! See: <https://github.com/astral-sh/uv/issues/17492>

use std::convert::Infallible;
use std::os::windows::io::AsRawHandle;
use std::os::windows::process::CommandExt;
use std::process::Command;

use windows::Win32::Foundation::{HANDLE, WAIT_OBJECT_0};
use windows::Win32::System::Threading::{
    CREATE_NO_WINDOW, GetExitCodeProcess, INFINITE, WaitForSingleObject,
};

use crate::{Job, install_ctrl_handler};

/// Spawns a child process using Job Objects to ensure proper cleanup.
///
/// When the parent process is terminated (e.g., by Task Scheduler), the child process
/// will also be terminated because it's associated with a Job Object that has
/// `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` set.
///
/// This function does not return on success - it calls `std::process::exit` with the
/// child's exit code.
#[allow(unsafe_code)]
pub fn spawn_child(cmd: &mut Command, hide_console: bool) -> std::io::Result<Infallible> {
    cmd.stdin(std::process::Stdio::inherit());

    if hide_console {
        cmd.creation_flags(CREATE_NO_WINDOW.0);
    }

    let child = cmd.spawn()?;

    let job = Job::new().map_err(|e| std::io::Error::other(e.to_string()))?;

    // SAFETY: `child` is alive so `as_raw_handle()` returns a valid handle.
    unsafe { job.assign_process(HANDLE(child.as_raw_handle())) }
        .map_err(|e| std::io::Error::other(e.to_string()))?;

    // Ignore control-C/control-Break/logout/etc.; the same event will be delivered
    // to the child, so we let them decide whether to exit or not.
    let _ = install_ctrl_handler();

    // SAFETY: `child` is alive so `as_raw_handle()` returns a valid handle.
    let wait_result = unsafe { WaitForSingleObject(HANDLE(child.as_raw_handle()), INFINITE) };
    if wait_result != WAIT_OBJECT_0 {
        return Err(std::io::Error::other(format!(
            "WaitForSingleObject failed with result: {wait_result:?}"
        )));
    }

    let mut exit_code = 0u32;
    // SAFETY: `child` is alive so `as_raw_handle()` returns a valid handle.
    unsafe { GetExitCodeProcess(HANDLE(child.as_raw_handle()), &raw mut exit_code) }
        .map_err(|e| std::io::Error::other(format!("Failed to get exit code: {e}")))?;

    // Close handles before exiting.
    drop(job);
    drop(child);

    #[allow(clippy::exit, clippy::cast_possible_wrap)]
    std::process::exit(exit_code as i32)
}
