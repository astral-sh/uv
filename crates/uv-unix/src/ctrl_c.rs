//! Unix Ctrl+C (SIGINT) handling.

use std::os::fd::AsRawFd;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use nix::fcntl::{FcntlArg, FdFlag, fcntl};
use nix::sys::signal;
use nix::unistd;

/// Error type for Ctrl+C handler operations.
#[derive(Debug)]
pub enum CtrlCError {
    /// A handler has already been registered.
    AlreadyRegistered,
    /// A system error occurred.
    System(std::io::Error),
}

impl std::fmt::Display for CtrlCError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyRegistered => write!(f, "a Ctrl+C handler is already registered"),
            Self::System(err) => write!(f, "failed to set Ctrl+C handler: {err}"),
        }
    }
}

impl std::error::Error for CtrlCError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::System(err) => Some(err),
            Self::AlreadyRegistered => None,
        }
    }
}

impl From<nix::Error> for CtrlCError {
    fn from(err: nix::Error) -> Self {
        Self::System(std::io::Error::from_raw_os_error(err as i32))
    }
}

/// Write-end of the self-pipe, written to by the signal handler.
///
/// Stored as a raw fd so it can be accessed from the async-signal-safe handler.
static PIPE_WRITE: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(-1);

/// Read-end of the self-pipe, read by the blocking thread.
///
/// Stored as a raw fd. Both ends of the pipe are leaked (never closed) to
/// ensure the file descriptors remain valid for the lifetime of the process.
static PIPE_READ: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(-1);

/// Whether a handler has already been registered.
static INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Signal handler that writes a byte to the self-pipe.
///
/// This function is async-signal-safe: it only calls `write(2)`.
#[allow(unsafe_code)]
extern "C" fn signal_handler(_: nix::libc::c_int) {
    let fd = PIPE_WRITE.load(Ordering::Relaxed);
    if fd >= 0 {
        // SAFETY: `fd` is a valid file descriptor for the write end of our pipe,
        // and `write` is async-signal-safe.
        unsafe {
            nix::libc::write(fd, [0u8].as_ptr().cast(), 1);
        }
    }
}

/// Sets `FD_CLOEXEC` on a file descriptor so it is not inherited by child processes.
fn set_cloexec(fd: &impl std::os::fd::AsFd) -> Result<(), nix::Error> {
    let flags = FdFlag::from_bits_retain(fcntl(fd, FcntlArg::F_GETFD)?);
    fcntl(fd, FcntlArg::F_SETFD(flags | FdFlag::FD_CLOEXEC))?;
    Ok(())
}

/// Registers a callback to be invoked when the user presses Ctrl+C (SIGINT).
///
/// Uses the self-pipe trick: installs a signal handler that writes to a pipe,
/// and spawns a thread that blocks reading from that pipe and calls `handler`.
///
/// Can only be called once. Returns an error if called again or if a system
/// error occurs.
#[allow(unsafe_code)]
pub fn on_ctrl_c<F>(handler: F) -> Result<(), CtrlCError>
where
    F: FnMut() + Send + 'static,
{
    if INITIALIZED.swap(true, Ordering::SeqCst) {
        return Err(CtrlCError::AlreadyRegistered);
    }

    // Create the self-pipe.
    let (pipe_read, pipe_write) = unistd::pipe()?;

    // Set close-on-exec so these fds are not inherited by child processes.
    set_cloexec(&pipe_read)?;
    set_cloexec(&pipe_write)?;

    // Store the raw fds. The `OwnedFd`s are leaked below to ensure the file
    // descriptors remain valid for the lifetime of the process.
    PIPE_READ.store(pipe_read.as_raw_fd(), Ordering::Relaxed);
    PIPE_WRITE.store(pipe_write.as_raw_fd(), Ordering::Relaxed);
    std::mem::forget(pipe_read);
    std::mem::forget(pipe_write);

    // Install the signal handler for SIGINT.
    let sig_handler = signal::SigHandler::Handler(signal_handler);
    let sig_action = signal::SigAction::new(
        sig_handler,
        signal::SaFlags::SA_RESTART,
        signal::SigSet::empty(),
    );

    // SAFETY: We're installing a valid signal handler. The handler is
    // async-signal-safe (only calls `write`).
    unsafe {
        signal::sigaction(signal::Signal::SIGINT, &sig_action)?;
    }

    // Spawn the blocking thread that reads from the pipe.
    thread::Builder::new()
        .name("ctrl-c".into())
        .spawn({
            let mut handler = handler;
            move || {
                let mut buf = [0u8; 1];
                loop {
                    let fd = PIPE_READ.load(Ordering::Relaxed);
                    // SAFETY: `fd` is a valid file descriptor for the read end
                    // of our pipe. It is leaked and never closed.
                    let result = unsafe { nix::libc::read(fd, buf.as_mut_ptr().cast(), 1) };
                    match result {
                        1 => handler(),
                        -1 if std::io::Error::last_os_error().raw_os_error()
                            == Some(nix::libc::EINTR) =>
                        {
                            // Interrupted by signal, retry.
                        }
                        _ => break,
                    }
                }
            }
        })
        .map_err(CtrlCError::System)?;

    Ok(())
}
