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
