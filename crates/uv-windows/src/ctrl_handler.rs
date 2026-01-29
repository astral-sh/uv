//! Windows console control handler for ignoring Ctrl+C.
//!
//! When running a wrapper process that spawns a child, we typically want to ignore
//! console control events (Ctrl+C, Ctrl+Break, etc.) in the wrapper and let the
//! child process handle them. This prevents the wrapper from exiting prematurely
//! while the child might want to handle the signal gracefully.

use windows::Win32::Foundation::TRUE;
use windows::Win32::System::Console::SetConsoleCtrlHandler;

/// Error type for control handler operations.
#[derive(Debug, Clone, Copy)]
pub struct CtrlHandlerError(i32);

impl CtrlHandlerError {
    /// Returns the Windows error code.
    #[must_use]
    pub const fn code(&self) -> i32 {
        self.0
    }
}

#[cfg(feature = "std")]
impl std::fmt::Display for CtrlHandlerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "failed to set console control handler (os error {})",
            self.0
        )
    }
}

#[cfg(feature = "std")]
impl std::error::Error for CtrlHandlerError {}

/// Installs a console control handler that ignores Ctrl+C/Ctrl+Break/etc.
///
/// This is useful for wrapper processes that spawn children - we want the child
/// to receive and handle the control signal, not the wrapper. Returning `TRUE`
/// from the handler tells Windows we've handled the signal (by ignoring it).
///
/// See: `distlib/PC/launcher.c::control_key_handler`
#[allow(unsafe_code)]
pub fn install_ctrl_handler() -> Result<(), CtrlHandlerError> {
    /// Handler that ignores all console control events.
    unsafe extern "system" fn handler(_: u32) -> windows::core::BOOL {
        TRUE
    }

    // SAFETY: We're registering a valid handler function.
    unsafe { SetConsoleCtrlHandler(Some(handler), true) }.map_err(|e| CtrlHandlerError(e.code().0))
}
