//! Windows console Ctrl+C handling.
//!
//! Provides two modes of handling Ctrl+C:
//!
//! - [`ignore_ctrl_c`]: Installs a handler that silently ignores Ctrl+C/Ctrl+Break events.
//!   Used by wrapper processes that spawn children — we want the child to receive and handle
//!   the control signal, not the wrapper.
//!
//! - [`on_ctrl_c`] (requires `std` feature): Installs a handler that runs a user-provided
//!   callback on Ctrl+C. Used by commands like `uv add` to revert in-flight changes when
//!   interrupted.

use windows::Win32::Foundation::TRUE;
use windows::Win32::System::Console::SetConsoleCtrlHandler;

/// Error type for control handler operations.
#[derive(Debug, Clone, Copy)]
pub enum CtrlCError {
    /// A handler has already been registered.
    AlreadyRegistered,
    /// A system error occurred.
    System(i32),
}

impl CtrlCError {
    /// Returns the Windows error code, or `0` if this is an `AlreadyRegistered` error.
    #[must_use]
    pub const fn code(&self) -> i32 {
        match self {
            Self::AlreadyRegistered => 0,
            Self::System(code) => *code,
        }
    }
}

#[cfg(feature = "std")]
impl std::fmt::Display for CtrlCError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyRegistered => write!(f, "a Ctrl+C handler is already registered"),
            Self::System(code) => {
                write!(f, "failed to set console control handler (os error {code})")
            }
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for CtrlCError {}

/// Installs a console control handler that ignores Ctrl+C/Ctrl+Break/etc.
///
/// This is useful for wrapper processes that spawn children — we want the child
/// to receive and handle the control signal, not the wrapper. Returning `TRUE`
/// from the handler tells Windows we've handled the signal (by ignoring it).
///
/// See: `distlib/PC/launcher.c::control_key_handler`
#[allow(unsafe_code)]
pub fn ignore_ctrl_c() -> Result<(), CtrlCError> {
    /// Handler that ignores all console control events.
    unsafe extern "system" fn handler(_: u32) -> windows::core::BOOL {
        TRUE
    }

    // SAFETY: We're registering a valid handler function.
    unsafe { SetConsoleCtrlHandler(Some(handler), true) }
        .map_err(|e| CtrlCError::System(e.code().0))
}

/// Registers a callback to be invoked when the user presses Ctrl+C.
///
/// Spawns a dedicated thread that blocks on a Windows semaphore. When Ctrl+C
/// is pressed, the OS-level handler posts to the semaphore, waking the thread
/// which then calls `handler`.
///
/// Can only be called once. Returns an error if called again or if a system
/// error occurs.
#[cfg(feature = "std")]
#[allow(unsafe_code)]
pub fn on_ctrl_c<F>(handler: F) -> Result<(), CtrlCError>
where
    F: FnMut() + Send + 'static,
{
    use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
    use std::thread;

    use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0};
    use windows::Win32::System::Threading::{
        CreateSemaphoreW, INFINITE, ReleaseSemaphore, WaitForSingleObject,
    };

    /// Global semaphore handle stored as a raw `isize` so it can be accessed
    /// lock-free from the OS console control handler.
    ///
    /// A value of `0` means uninitialized; any other value is a valid `HANDLE`.
    static SEMAPHORE: AtomicIsize = AtomicIsize::new(0);

    /// Whether a handler has already been registered.
    static INITIALIZED: AtomicBool = AtomicBool::new(false);

    /// Console control handler that posts to the semaphore.
    ///
    /// This is called by the OS in a special thread context — it must not
    /// block or panic. We use an atomic load to read the semaphore handle
    /// without any locking.
    unsafe extern "system" fn os_handler(_: u32) -> windows::core::BOOL {
        let raw = SEMAPHORE.load(Ordering::Acquire);
        if raw != 0 {
            // SAFETY: The semaphore handle is initialized before this handler
            // is registered and is never closed while the handler is active.
            let _ = unsafe { ReleaseSemaphore(HANDLE(raw as *mut _), 1, None) };
        }
        TRUE
    }

    if INITIALIZED.swap(true, Ordering::SeqCst) {
        return Err(CtrlCError::AlreadyRegistered);
    }

    // SAFETY: Creating a semaphore with no security attributes and no name.
    let semaphore = unsafe { CreateSemaphoreW(None, 0, i32::MAX, None) }
        .map_err(|e| CtrlCError::System(e.code().0))?;

    SEMAPHORE.store(semaphore.0 as isize, Ordering::Release);

    // SAFETY: We're registering a valid handler function.
    unsafe { SetConsoleCtrlHandler(Some(os_handler), true) }.map_err(|e| {
        // Clean up the semaphore on failure.
        let raw = SEMAPHORE.swap(0, Ordering::SeqCst);
        if raw != 0 {
            // SAFETY: The semaphore handle is valid and we created it above.
            unsafe {
                let _ = CloseHandle(HANDLE(raw as *mut _));
            }
        }
        INITIALIZED.store(false, Ordering::SeqCst);
        CtrlCError::System(e.code().0)
    })?;

    // Spawn the blocking thread.
    thread::Builder::new()
        .name("ctrl-c".into())
        .spawn({
            let mut handler = handler;
            move || loop {
                let raw = SEMAPHORE.load(Ordering::Acquire);
                // SAFETY: The semaphore handle is valid and we wait indefinitely.
                let result = unsafe { WaitForSingleObject(HANDLE(raw as *mut _), INFINITE) };
                if result == WAIT_OBJECT_0 {
                    handler();
                }
            }
        })
        .map_err(|e| CtrlCError::System(e.raw_os_error().unwrap_or(0)))?;

    Ok(())
}
