//! Cross-platform Ctrl+C handling.
//!
//! Registers a signal handler (SIGINT on Unix, console control handler on Windows)
//! and invokes a user-provided callback when Ctrl+C is pressed.

/// Platform-specific error type for Ctrl+C handler operations.
#[cfg(unix)]
pub type CtrlCError = uv_unix::CtrlCError;

/// Platform-specific error type for Ctrl+C handler operations.
#[cfg(windows)]
pub type CtrlCError = uv_windows::CtrlCError;

/// Registers a callback to be invoked when the user presses Ctrl+C.
///
/// Spawns a dedicated thread that waits for the signal and calls `handler`.
/// Can only be called once per process.
///
/// # Errors
///
/// Returns an error if a handler has already been registered or if a system
/// error occurs during setup.
#[cfg(unix)]
pub fn on_ctrl_c<F>(handler: F) -> Result<(), CtrlCError>
where
    F: FnMut() + Send + 'static,
{
    uv_unix::on_ctrl_c(handler)
}

/// Registers a callback to be invoked when the user presses Ctrl+C.
///
/// Spawns a dedicated thread that waits for the signal and calls `handler`.
/// Can only be called once per process.
///
/// # Errors
///
/// Returns an error if a handler has already been registered or if a system
/// error occurs during setup.
#[cfg(windows)]
pub fn on_ctrl_c<F>(handler: F) -> Result<(), CtrlCError>
where
    F: FnMut() + Send + 'static,
{
    uv_windows::on_ctrl_c(handler)
}
