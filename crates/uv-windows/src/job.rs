//! Windows Job Objects for process lifecycle management.
//!
//! Job Objects allow grouping processes together and applying limits. The key feature
//! used here is `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`, which ensures that when the job
//! handle is closed (e.g., when the parent process exits), all processes in the job
//! are terminated.
//!
//! This is essential for wrapper processes (like `uvx.exe` or the Python trampoline)
//! to ensure child processes don't become orphaned when the wrapper is killed.

use core::ffi::c_void;

use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOB_OBJECT_LIMIT_SILENT_BREAKAWAY_OK, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JobObjectExtendedLimitInformation, QueryInformationJobObject, SetInformationJobObject,
};

/// Error type for job object operations.
#[derive(Debug, Clone, Copy)]
pub enum JobError {
    /// Failed to create the job object.
    Create(i32),
    /// Failed to query job object information.
    Query(i32),
    /// Failed to set job object information.
    Set(i32),
    /// Failed to assign process to job object.
    Assign(i32),
}

/// The HRESULT code for `ERROR_ACCESS_DENIED` (0x80070005).
const E_ACCESSDENIED: i32 = -2_147_024_891;

impl JobError {
    /// Returns the Windows error code associated with this error.
    #[must_use]
    pub const fn code(&self) -> i32 {
        match *self {
            Self::Create(code) | Self::Query(code) | Self::Set(code) | Self::Assign(code) => code,
        }
    }

    /// Returns a static description of the error kind.
    #[must_use]
    pub const fn message(&self) -> &'static str {
        match self {
            Self::Create(_) => "failed to create job object",
            Self::Query(_) => "failed to query job object information",
            Self::Set(_) => "failed to set job object information",
            Self::Assign(_) => "failed to assign process to job object",
        }
    }

    /// Returns `true` if this is an `Assign` error caused by `ERROR_ACCESS_DENIED`.
    ///
    /// In practice this can happen in two expected scenarios:
    /// - The child is already in a parent-managed job object that does not allow
    ///   nesting (for example, certain CI or scheduler environments).
    /// - A timing race where the child transitions state before the post-spawn
    ///   `AssignProcessToJobObject` call (most visible with very short-lived commands).
    ///
    /// Both cases are safe to treat as non-fatal in callers that can continue
    /// without owning a dedicated job object.
    #[must_use]
    pub const fn is_access_denied(&self) -> bool {
        matches!(self, Self::Assign(code) if *code == E_ACCESSDENIED)
    }
}

#[cfg(feature = "std")]
impl std::fmt::Display for JobError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} (os error {})", self.message(), self.code())
    }
}

#[cfg(feature = "std")]
impl std::error::Error for JobError {}

/// A Windows Job Object configured to terminate child processes when closed.
///
/// When a `Job` is dropped, the job handle is closed. If `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`
/// is set (which [`Job::new`] does by default), all processes assigned to the job will be
/// terminated.
pub struct Job {
    handle: HANDLE,
}

impl Job {
    /// Creates a new Job Object configured for child process lifecycle management.
    ///
    /// The job is configured with:
    /// - `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`: Terminate all processes when job handle closes
    /// - `JOB_OBJECT_LIMIT_SILENT_BREAKAWAY_OK`: Allow child processes to break away if needed
    #[allow(unsafe_code)]
    pub fn new() -> Result<Self, JobError> {
        // SAFETY: CreateJobObjectW with None parameters creates an unnamed job object.
        // This is a standard Windows API call with no special requirements.
        let handle =
            unsafe { CreateJobObjectW(None, None) }.map_err(|e| JobError::Create(e.code().0))?;

        let job = Self { handle };
        job.configure_limits()?;
        Ok(job)
    }

    /// Assigns a process to this job object.
    ///
    /// Once assigned, the process will be terminated when the job handle is closed
    /// (assuming `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` is set).
    ///
    /// # Safety
    ///
    /// The caller must ensure `process_handle` is a valid process handle.
    #[allow(unsafe_code)]
    pub unsafe fn assign_process(&self, process_handle: HANDLE) -> Result<(), JobError> {
        // SAFETY: Caller guarantees process_handle is valid. self.handle is valid
        // because we only create it via new() and don't expose mutation.
        unsafe { AssignProcessToJobObject(self.handle, process_handle) }
            .map_err(|e| JobError::Assign(e.code().0))
    }

    /// Returns the raw job handle.
    ///
    /// The returned handle is owned by this `Job` and will be closed when the `Job`
    /// is dropped. The caller must not close or otherwise invalidate this handle.
    #[must_use]
    pub const fn as_raw_handle(&self) -> HANDLE {
        self.handle
    }

    /// Configures the job object limits.
    #[allow(unsafe_code)]
    fn configure_limits(&self) -> Result<(), JobError> {
        let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        let info_size = u32::try_from(size_of_val(&info)).expect("job info size fits in u32");

        // SAFETY: We pass a valid job handle, the correct information class,
        // a properly sized buffer, and the buffer size.
        unsafe {
            QueryInformationJobObject(
                Some(self.handle),
                JobObjectExtendedLimitInformation,
                (&raw mut info).cast::<c_void>(),
                info_size,
                None,
            )
        }
        .map_err(|e| JobError::Query(e.code().0))?;

        // Set the limits we need
        info.BasicLimitInformation.LimitFlags |= JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        info.BasicLimitInformation.LimitFlags |= JOB_OBJECT_LIMIT_SILENT_BREAKAWAY_OK;

        // SAFETY: We pass a valid job handle, the correct information class,
        // a properly initialized info struct, and its size.
        unsafe {
            SetInformationJobObject(
                self.handle,
                JobObjectExtendedLimitInformation,
                (&raw const info).cast::<c_void>(),
                info_size,
            )
        }
        .map_err(|e| JobError::Set(e.code().0))
    }
}

impl Drop for Job {
    #[allow(unsafe_code)]
    fn drop(&mut self) {
        // SAFETY: self.handle is valid and we're done using it.
        // Ignoring the result is fine - there's nothing we can do if close fails.
        let _ = unsafe { CloseHandle(self.handle) };
    }
}

#[cfg(all(test, feature = "std"))]
mod tests {
    use std::os::windows::io::{AsHandle, AsRawHandle};
    use std::process::Command;

    use windows::Win32::Foundation::HANDLE;

    use super::*;

    /// Creates a "restrictive" job that does NOT set `JOB_OBJECT_LIMIT_SILENT_BREAKAWAY_OK`
    /// or `JOB_OBJECT_LIMIT_BREAKAWAY_OK`, preventing processes from being assigned to
    /// nested child jobs.
    #[allow(unsafe_code)]
    fn create_restrictive_job() -> Job {
        // SAFETY: Creating an unnamed job object with default security.
        let handle =
            unsafe { CreateJobObjectW(None, None) }.expect("failed to create restrictive job");
        let job = Job { handle };

        let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        let info_size = u32::try_from(size_of_val(&info)).expect("job info size fits in u32");

        // Only set KILL_ON_JOB_CLOSE — deliberately omit SILENT_BREAKAWAY_OK
        // and BREAKAWAY_OK so the job does not permit nesting.
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;

        // SAFETY: Valid job handle, correct info class, properly sized buffer.
        unsafe {
            SetInformationJobObject(
                job.handle,
                JobObjectExtendedLimitInformation,
                (&raw const info).cast::<core::ffi::c_void>(),
                info_size,
            )
        }
        .expect("failed to set restrictive job limits");

        job
    }

    /// Demonstrates one expected `E_ACCESSDENIED` path from
    /// `AssignProcessToJobObject`.
    ///
    /// If a process is already in a restrictive job that does not permit
    /// nesting, assigning it to a second job fails with `ERROR_ACCESS_DENIED`.
    #[test]
    #[allow(unsafe_code)]
    fn assign_to_second_job_can_fail_with_access_denied() {
        // Create a restrictive job (no breakaway, no nesting).
        let restrictive_job = create_restrictive_job();

        // Spawn a child process that will live long enough for us to manipulate.
        let child = Command::new("cmd.exe")
            .args(["/C", "timeout /T 30 /NOBREAK >NUL"])
            .spawn()
            .expect("failed to spawn child process");

        let child_handle = HANDLE(child.as_handle().as_raw_handle());

        // Assign the child to the restrictive job.
        // SAFETY: child_handle is a valid process handle from the spawn above.
        unsafe { restrictive_job.assign_process(child_handle) }
            .expect("failed to assign child to restrictive job");

        // Now create a second job and try to assign the same child.
        // This should fail with E_ACCESSDENIED because the restrictive job
        // doesn't permit nesting.
        let second_job = Job::new().expect("failed to create second job");
        // SAFETY: child_handle is still a valid process handle.
        let result = unsafe { second_job.assign_process(child_handle) };

        let err = result.expect_err(
            "assigning to a second job should fail when the first job doesn't permit nesting",
        );
        assert!(
            err.is_access_denied(),
            "expected ACCESS_DENIED error, got: {err} (code: {})",
            err.code()
        );
    }
}
