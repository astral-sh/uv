//! Windows-specific process spawning with Job Objects.
//!
//! This module provides process spawning functionality that ensures child processes
//! are properly terminated when the parent process is killed. This is important for
//! tools like Task Scheduler that terminate wrapper processes.
//!
//! See: <https://github.com/astral-sh/uv/issues/17492>

use std::convert::Infallible;
use std::os::windows::io::{AsHandle, AsRawHandle, BorrowedHandle};
use std::os::windows::process::CommandExt;
use std::process::{Child, Command};

use windows::Win32::Foundation::{HANDLE, WAIT_OBJECT_0};
use windows::Win32::System::Threading::{
    CREATE_NO_WINDOW, CREATE_SUSPENDED, GetExitCodeProcess, INFINITE, WaitForSingleObject,
};

use crate::job::JobError;
use crate::{Job, install_ctrl_handler};

#[allow(unsafe_code)]
#[link(name = "ntdll")]
unsafe extern "system" {
    fn NtResumeProcess(processhandle: HANDLE) -> i32;
}

#[allow(unsafe_code)]
fn resume_process(process: HANDLE) -> std::io::Result<()> {
    // SAFETY: `process` is a valid process handle from `std::process::Child`.
    let status = unsafe { NtResumeProcess(process) };
    if status < 0 {
        return Err(std::io::Error::other(format!(
            "NtResumeProcess failed with status {status:#x}"
        )));
    }
    Ok(())
}

/// A child process supervised by a [`Job`] object.
///
/// The [`BorrowedHandle`] ties the lifetime of the job to the child process,
/// ensuring the job cannot outlive the child's process handle.
struct SupervisedChild<'a> {
    _job: Job,
    handle: BorrowedHandle<'a>,
}

#[allow(unsafe_code)]
impl<'a> SupervisedChild<'a> {
    /// Creates a new supervised child by assigning it to a job object.
    fn new(child: &'a Child) -> Result<Self, JobError> {
        let job = Job::new()?;
        let handle = child.as_handle();
        // SAFETY: The borrowed handle is valid for `'a` because we hold a
        // reference to `child`.
        unsafe { job.assign_process(HANDLE(handle.as_raw_handle())) }?;
        Ok(Self { _job: job, handle })
    }

    /// Returns the raw Windows `HANDLE` for the child process.
    fn raw_handle(&self) -> HANDLE {
        HANDLE(self.handle.as_raw_handle())
    }
}

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

    // Spawn suspended so job assignment happens before child execution.
    // This avoids post-spawn assignment races with short-lived children.
    //
    // We use `NtResumeProcess` later because `std::process::Child` does not
    // expose the primary thread handle needed for `ResumeThread`.
    let mut creation_flags = CREATE_SUSPENDED;
    if hide_console {
        creation_flags |= CREATE_NO_WINDOW;
    }
    cmd.creation_flags(creation_flags.0);

    let mut child = cmd.spawn()?;

    let supervised = match SupervisedChild::new(&child) {
        Ok(supervised) => supervised,
        Err(err) => {
            let _ = child.kill();
            return Err(std::io::Error::other(err.to_string()));
        }
    };

    resume_process(supervised.raw_handle())?;

    // Ignore control-C/control-Break/logout/etc.; the same event will be delivered
    // to the child, so we let them decide whether to exit or not.
    let _ = install_ctrl_handler();

    // SAFETY: The handle is valid because `supervised` borrows `child`.
    let wait_result = unsafe { WaitForSingleObject(supervised.raw_handle(), INFINITE) };
    if wait_result != WAIT_OBJECT_0 {
        return Err(std::io::Error::other(format!(
            "WaitForSingleObject failed with result: {wait_result:?}"
        )));
    }

    let mut exit_code = 0u32;
    // SAFETY: The handle is valid because `supervised` borrows `child`.
    unsafe { GetExitCodeProcess(supervised.raw_handle(), &raw mut exit_code) }
        .map_err(|e| std::io::Error::other(format!("Failed to get exit code: {e}")))?;

    #[allow(clippy::exit, clippy::cast_possible_wrap)]
    std::process::exit(exit_code as i32)
}
