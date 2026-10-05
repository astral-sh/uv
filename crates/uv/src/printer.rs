use indicatif::ProgressDrawTarget;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Printer {
    /// A printer that suppresses all output.
    Silent,
    /// A printer that suppresses most output, but preserves "important" stdout.
    Quiet,
    /// A printer that prints to standard streams (e.g., stdout).
    Default,
    /// A printer that prints all output, including debug messages.
    Verbose,
    /// A printer that prints to standard streams, excluding all progress outputs
    NoProgress,
}

impl Printer {
    /// Create a printer from the global output settings.
    pub(crate) fn new(quiet: u8, verbose: u8, no_progress: bool) -> Self {
        if quiet == 1 {
            Self::Quiet
        } else if quiet > 1 {
            Self::Silent
        } else if verbose > 0 {
            Self::Verbose
        } else if no_progress {
            Self::NoProgress
        } else {
            Self::Default
        }
    }

    /// Return whether this printer suppresses progress output.
    pub(crate) const fn suppresses_progress(self) -> bool {
        match self {
            Self::Silent => true,
            Self::Quiet => true,
            Self::Default => false,
            // Confusingly, hide the progress bar when in verbose mode.
            // Otherwise, it gets interleaved with debug messages.
            Self::Verbose => true,
            Self::NoProgress => true,
        }
    }

    /// Return the [`ProgressDrawTarget`] for this printer.
    pub(crate) fn target(self) -> ProgressDrawTarget {
        if self.suppresses_progress() {
            ProgressDrawTarget::hidden()
        } else {
            ProgressDrawTarget::stderr()
        }
    }

    /// Return the [`Stdout`] for this printer.
    #[allow(dead_code, reason = "to be adopted incrementally")]
    pub(crate) fn stdout_important(self) -> Stdout {
        match self {
            Self::Silent => Stdout::Disabled,
            Self::Quiet => Stdout::Enabled,
            Self::Default => Stdout::Enabled,
            Self::Verbose => Stdout::Enabled,
            Self::NoProgress => Stdout::Enabled,
        }
    }

    /// Return the [`Stdout`] for this printer.
    pub(crate) fn stdout(self) -> Stdout {
        match self {
            Self::Silent => Stdout::Disabled,
            Self::Quiet => Stdout::Disabled,
            Self::Default => Stdout::Enabled,
            Self::Verbose => Stdout::Enabled,
            Self::NoProgress => Stdout::Enabled,
        }
    }

    /// Return the [`Stderr`] for this printer.
    pub(crate) fn stderr_important(self) -> Stderr {
        match self {
            Self::Silent => Stderr::Disabled,
            Self::Quiet => Stderr::Enabled,
            Self::Default => Stderr::Enabled,
            Self::Verbose => Stderr::Enabled,
            Self::NoProgress => Stderr::Enabled,
        }
    }

    /// Return the [`Stderr`] for this printer.
    pub(crate) fn stderr(self) -> Stderr {
        match self {
            Self::Silent => Stderr::Disabled,
            Self::Quiet => Stderr::Disabled,
            Self::Default => Stderr::Enabled,
            Self::Verbose => Stderr::Enabled,
            Self::NoProgress => Stderr::Enabled,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Stdout {
    Enabled,
    Disabled,
}

impl std::fmt::Write for Stdout {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        match self {
            Self::Enabled => {
                #[cfg(unix)]
                write_resilient(
                    &mut anstream::stdout().lock(),
                    libc::STDOUT_FILENO,
                    s.as_bytes(),
                );
                #[cfg(not(unix))]
                write_resilient(&mut anstream::stdout().lock(), s.as_bytes());
            }
            Self::Disabled => {}
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Stderr {
    Enabled,
    Disabled,
}

impl std::fmt::Write for Stderr {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        match self {
            Self::Enabled => {
                #[cfg(unix)]
                write_resilient(
                    &mut anstream::stderr().lock(),
                    libc::STDERR_FILENO,
                    s.as_bytes(),
                );
                #[cfg(not(unix))]
                write_resilient(&mut anstream::stderr().lock(), s.as_bytes());
            }
            Self::Disabled => {}
        }

        Ok(())
    }
}

#[cfg(unix)]
#[allow(unsafe_code)]
fn wait_writable(fd: std::os::fd::RawFd) -> bool {
    let mut pfd = libc::pollfd {
        fd,
        events: libc::POLLOUT,
        revents: 0,
    };
    // SAFETY: &raw mut pfd points to a valid libc::pollfd stack structure with length 1.
    let ret = unsafe { libc::poll(&raw mut pfd, 1, 500) };
    if ret < 0 {
        // Interrupted by signal or poll failure. On EINTR, sleep briefly to avoid a tight
        // spin loop if signals arrive repeatedly; on any other error, stop retrying.
        let err = std::io::Error::last_os_error();
        if err.kind() == std::io::ErrorKind::Interrupted {
            std::thread::sleep(std::time::Duration::from_millis(1));
            return true;
        }
        return false;
    }
    // If the fd encountered a fatal condition (error/hangup/invalid) without being writable, stop retrying.
    if ret > 0
        && (pfd.revents & (libc::POLLERR | libc::POLLNVAL | libc::POLLHUP) != 0)
        && (pfd.revents & libc::POLLOUT == 0)
    {
        return false;
    }
    true
}

fn write_resilient<W: std::io::Write>(
    mut writer: W,
    #[cfg(unix)] fd: std::os::fd::RawFd,
    mut bytes: &[u8],
) {
    while !bytes.is_empty() {
        match writer.write(bytes) {
            Ok(0) => break,
            Ok(n) => bytes = &bytes[n..],
            Err(err) if err.kind() == std::io::ErrorKind::Interrupted => {}
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                #[cfg(unix)]
                if !wait_writable(fd) {
                    break;
                }
                #[cfg(not(unix))]
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            Err(err) if err.kind() == std::io::ErrorKind::BrokenPipe => break,
            Err(_) => break,
        }
    }

    while let Err(err) = writer.flush() {
        if err.kind() == std::io::ErrorKind::Interrupted {
            continue;
        }
        if err.kind() == std::io::ErrorKind::WouldBlock {
            #[cfg(unix)]
            if !wait_writable(fd) {
                break;
            }
            #[cfg(not(unix))]
            std::thread::sleep(std::time::Duration::from_millis(1));
            continue;
        }
        break;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(unix)]
    #[allow(unsafe_code)]
    #[expect(
        clippy::disallowed_types,
        reason = "fs_err does not implement FromRawFd for socketpair"
    )]
    fn non_blocking_write_resilient() {
        use std::io::Read;
        use std::os::fd::{AsRawFd, FromRawFd};

        let mut fds = [0; 2];
        // SAFETY: socketpair is called with valid AF_UNIX domain, SOCK_STREAM type, and pointer to fds buffer of size 2.
        assert_eq!(
            unsafe { libc::socketpair(libc::AF_UNIX, libc::SOCK_STREAM, 0, fds.as_mut_ptr()) },
            0
        );
        // SAFETY: fds contains valid, open file descriptors from successful socketpair call.
        let mut parent = unsafe { std::fs::File::from_raw_fd(fds[0]) };
        let mut child = unsafe { std::fs::File::from_raw_fd(fds[1]) };

        // Set small send buffer so EAGAIN happens quickly.
        let sndbuf: libc::c_int = 2048;
        let socklen = libc::socklen_t::try_from(std::mem::size_of::<libc::c_int>()).unwrap();
        // SAFETY: setsockopt called with valid socket fd, SOL_SOCKET level, SO_SNDBUF option and valid buffer.
        assert_eq!(
            unsafe {
                libc::setsockopt(
                    child.as_raw_fd(),
                    libc::SOL_SOCKET,
                    libc::SO_SNDBUF,
                    std::ptr::addr_of!(sndbuf).cast(),
                    socklen,
                )
            },
            0
        );

        // Set O_NONBLOCK on child.
        // SAFETY: fcntl called with valid socket fd.
        let flags = unsafe { libc::fcntl(child.as_raw_fd(), libc::F_GETFL, 0) };
        assert_eq!(
            unsafe { libc::fcntl(child.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) },
            0
        );

        let data = "hello world\n".repeat(2000);
        let data_bytes = data.into_bytes();

        let reader_handle = std::thread::spawn(move || {
            // Sleep briefly so writer encounters WouldBlock
            std::thread::sleep(std::time::Duration::from_millis(50));
            let mut read_bytes = Vec::new();
            parent.read_to_end(&mut read_bytes).unwrap();
            read_bytes
        });

        let fd = child.as_raw_fd();
        write_resilient(&mut child, fd, &data_bytes);
        drop(child);

        let received = reader_handle.join().unwrap();
        assert_eq!(received, data_bytes);
    }

    #[test]
    #[cfg(unix)]
    #[allow(unsafe_code)]
    #[expect(
        clippy::disallowed_types,
        reason = "fs_err does not implement FromRawFd for socketpair"
    )]
    fn non_blocking_broken_pipe_resilient() {
        use std::os::fd::{AsRawFd, FromRawFd};

        let mut fds = [0; 2];
        // SAFETY: socketpair is called with valid AF_UNIX domain, SOCK_STREAM type, and pointer to fds buffer of size 2.
        assert_eq!(
            unsafe { libc::socketpair(libc::AF_UNIX, libc::SOCK_STREAM, 0, fds.as_mut_ptr()) },
            0
        );
        // SAFETY: fds contains valid, open file descriptors from successful socketpair call.
        let parent = unsafe { std::fs::File::from_raw_fd(fds[0]) };
        let mut child = unsafe { std::fs::File::from_raw_fd(fds[1]) };

        // SAFETY: fcntl called with valid socket fd.
        let flags = unsafe { libc::fcntl(child.as_raw_fd(), libc::F_GETFL, 0) };
        assert_eq!(
            unsafe { libc::fcntl(child.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) },
            0
        );

        // Close the read end immediately so writing hits EPIPE (BrokenPipe).
        drop(parent);

        let data = "hello world\n".repeat(100);
        let fd = child.as_raw_fd();
        // Must not panic on broken pipe:
        write_resilient(&mut child, fd, data.as_bytes());
    }
}
