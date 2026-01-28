//! Windows-specific process spawning with Job Objects.
//!
//! This module provides process spawning functionality that ensures child processes
//! are properly terminated when the parent process is killed. This is important for
//! tools like Task Scheduler that terminate wrapper processes.
//!
//! See: <https://github.com/astral-sh/uv/issues/17492>

use std::convert::Infallible;
use std::ffi::c_void;
use std::os::windows::io::IntoRawHandle;
use std::os::windows::process::CommandExt;
use std::process::Command;

use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0};
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOB_OBJECT_LIMIT_SILENT_BREAKAWAY_OK, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JobObjectExtendedLimitInformation, QueryInformationJobObject, SetInformationJobObject,
};
use windows::Win32::System::Threading::{GetExitCodeProcess, INFINITE, WaitForSingleObject};

/// Process creation flag to prevent creating a console window.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Size of the job object info struct, computed at compile time.
/// The struct is a fixed Windows type (~64 bytes) that always fits in u32.
#[allow(clippy::cast_possible_truncation)] // Guarded by the assert above
const JOB_INFO_SIZE: u32 = {
    const SIZE: usize = size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>();
    assert!(
        SIZE <= u32::MAX as usize,
        "job info struct too large for u32"
    );
    SIZE as u32
};

/// Spawns a child process using Job Objects to ensure proper cleanup.
///
/// When the parent process is terminated (e.g., by Task Scheduler), the child process
/// will also be terminated because it's associated with a Job Object that has
/// `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` set.
#[allow(unsafe_code)]
pub fn spawn_child(cmd: &mut Command, hide_console: bool) -> std::io::Result<Infallible> {
    cmd.stdin(std::process::Stdio::inherit());

    if hide_console {
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let child = cmd.spawn()?;

    // SAFETY: `into_raw_handle()` returns a valid process handle that we own.
    // We are responsible for closing it, which we do at the end of this function.
    let child_handle = HANDLE(child.into_raw_handle());

    // SAFETY: CreateJobObjectW with None parameters creates an unnamed job object.
    let job = unsafe { CreateJobObjectW(None, None) }
        .map_err(|e| std::io::Error::other(format!("Failed to create job object: {e}")))?;

    // Query current job information
    let mut job_info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    let mut _retlen = 0u32;
    // SAFETY: We pass a valid job handle, the correct information class, a properly
    // sized buffer, and the buffer size.
    unsafe {
        QueryInformationJobObject(
            Some(job),
            JobObjectExtendedLimitInformation,
            (&raw mut job_info).cast::<c_void>(),
            JOB_INFO_SIZE,
            Some(&raw mut _retlen),
        )
    }
    .map_err(|e| std::io::Error::other(format!("Failed to query job object information: {e}")))?;

    // Set job object limits
    job_info.BasicLimitInformation.LimitFlags |= JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    job_info.BasicLimitInformation.LimitFlags |= JOB_OBJECT_LIMIT_SILENT_BREAKAWAY_OK;

    // SAFETY: We pass a valid job handle, the correct information class, a properly
    // initialized job_info struct, and its size.
    unsafe {
        SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            (&raw const job_info).cast::<c_void>(),
            JOB_INFO_SIZE,
        )
    }
    .map_err(|e| std::io::Error::other(format!("Failed to set job object information: {e}")))?;

    // SAFETY: Both job and child_handle are valid handles we created/obtained above.
    unsafe { AssignProcessToJobObject(job, child_handle) }.map_err(|e| {
        std::io::Error::other(format!("Failed to assign process to job object: {e}"))
    })?;

    // SAFETY: child_handle is a valid process handle. INFINITE means wait forever.
    let wait_result = unsafe { WaitForSingleObject(child_handle, INFINITE) };
    if wait_result != WAIT_OBJECT_0 {
        return Err(std::io::Error::other(format!(
            "WaitForSingleObject failed with result: {wait_result:?}"
        )));
    }

    // SAFETY: child_handle is valid and exit_code points to valid memory.
    let mut exit_code = 0u32;
    unsafe { GetExitCodeProcess(child_handle, &raw mut exit_code) }
        .map_err(|e| std::io::Error::other(format!("Failed to get exit code: {e}")))?;

    // SAFETY: Both handles are valid and we're done using them.
    let _ = unsafe { CloseHandle(child_handle) };
    let _ = unsafe { CloseHandle(job) };

    #[allow(clippy::exit, clippy::cast_possible_wrap)]
    std::process::exit(exit_code as i32)
}
