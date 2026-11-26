//! Windows-specific process spawning with Job Objects.
//!
//! This module provides process spawning functionality that ensures child processes
//! are properly terminated when the parent process is killed. This is important for
//! tools like Task Scheduler that terminate wrapper processes.
//!
//! See: <https://github.com/astral-sh/uv/issues/17492>

use std::convert::Infallible;
use std::os::windows::io::{AsHandle, AsRawHandle, BorrowedHandle, FromRawHandle, OwnedHandle};
use std::os::windows::process::CommandExt;
use std::process::{Child, Command};

use windows::Win32::Foundation::{HANDLE, WAIT_OBJECT_0};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
};
use windows::Win32::System::Threading::{
    CREATE_NO_WINDOW, CREATE_SUSPENDED, GetExitCodeProcess, INFINITE, OpenThread,
    PROCESS_CREATION_FLAGS, ResumeThread, THREAD_SUSPEND_RESUME, WaitForSingleObject,
};
use windows::Win32::UI::WindowsAndMessaging::AllowSetForegroundWindow;

use crate::job::JobError;
use crate::{Job, install_ctrl_handler};

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

/// Allows the process with the given ID to bring its windows to the foreground.
///
/// This has no effect if the current process is not allowed to take the foreground itself.
#[allow(unsafe_code)]
fn allow_set_foreground_window(process_id: u32) -> windows::core::Result<()> {
    // SAFETY: `AllowSetForegroundWindow` has no memory safety preconditions.
    unsafe { AllowSetForegroundWindow(process_id) }
}

/// Resumes the threads of a process that was created with [`CREATE_SUSPENDED`].
#[allow(unsafe_code)]
fn resume_process(process_id: u32) -> windows::core::Result<()> {
    // SAFETY: The snapshot handle is valid and owned by `snapshot`, which closes it on drop.
    let snapshot =
        unsafe { OwnedHandle::from_raw_handle(CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0)?.0) };
    let mut entry = THREADENTRY32 {
        dwSize: u32::try_from(size_of::<THREADENTRY32>()).expect("thread entry size fits in u32"),
        ..THREADENTRY32::default()
    };
    // SAFETY: `snapshot` is a valid thread snapshot and `entry.dwSize` is set.
    let mut next = unsafe { Thread32First(HANDLE(snapshot.as_raw_handle()), &raw mut entry) };
    while next.is_ok() {
        if entry.th32OwnerProcessID == process_id {
            // SAFETY: The thread handle is valid and owned by `thread`, which closes it on drop.
            let thread = unsafe {
                OwnedHandle::from_raw_handle(
                    OpenThread(THREAD_SUSPEND_RESUME, false, entry.th32ThreadID)?.0,
                )
            };
            // SAFETY: `thread` is a valid handle with `THREAD_SUSPEND_RESUME` access.
            if unsafe { ResumeThread(HANDLE(thread.as_raw_handle())) } == u32::MAX {
                return Err(windows::core::Error::from_win32());
            }
        }
        // SAFETY: `snapshot` is a valid thread snapshot and `entry.dwSize` is set.
        next = unsafe { Thread32Next(HANDLE(snapshot.as_raw_handle()), &raw mut entry) };
    }
    Ok(())
}

/// Spawns a child process using Job Objects to ensure proper cleanup.
///
/// When the parent process is terminated (e.g., by Task Scheduler), the child process
/// will also be terminated because it's associated with a Job Object that has
/// `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` set.
///
/// If `allow_foreground` is set, the child may bring its windows to the foreground when the
/// current process may do so.
///
/// This function does not return on success - it calls `std::process::exit` with the
/// child's exit code.
#[allow(unsafe_code)]
pub fn spawn_child(
    cmd: &mut Command,
    hide_console: bool,
    allow_foreground: bool,
) -> std::io::Result<Infallible> {
    cmd.stdin(std::process::Stdio::inherit());

    let mut creation_flags = PROCESS_CREATION_FLAGS(0);
    if hide_console {
        creation_flags |= CREATE_NO_WINDOW;
    }
    if allow_foreground {
        creation_flags |= CREATE_SUSPENDED;
    }
    cmd.creation_flags(creation_flags.0);

    let mut child = cmd.spawn()?;

    if allow_foreground {
        // The child is still suspended, so it is allowed to take the foreground before it can
        // create a window. If this fails, its windows may open in the background.
        let _ = allow_set_foreground_window(child.id());
        if let Err(err) = resume_process(child.id()) {
            let _ = child.kill();
            return Err(std::io::Error::other(format!(
                "Failed to resume the child process: {err}"
            )));
        }
    }

    let supervised =
        SupervisedChild::new(&child).map_err(|e| std::io::Error::other(e.to_string()))?;

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

    #[expect(clippy::exit, clippy::cast_possible_wrap)]
    std::process::exit(exit_code as i32)
}
