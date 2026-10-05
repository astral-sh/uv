use std::io::{self, ErrorKind};
use std::time::Duration;

use anstream::print;
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
                print!("{s}");
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
            Self::Enabled => write_retrying(&mut anstream::stderr(), s.as_bytes()),
            Self::Disabled => {}
        }

        Ok(())
    }
}

/// The delay between attempts to write to a stream that is not ready to accept more data.
const WRITE_RETRY_DELAY: Duration = Duration::from_millis(1);

/// Write `buffer` to `writer`, retrying if the stream is non-blocking and temporarily full.
///
/// Parent processes like Node.js and Bun may set `O_NONBLOCK` on stdio shared with child processes,
/// in which case a write returns [`ErrorKind::WouldBlock`] (`EAGAIN`) whenever the reader falls
/// behind. Unlike `eprint!`, which panics on any error, we wait for the reader to catch up.
///
/// Any other error is ignored, since there is nothing useful to do if diagnostics cannot be
/// written.
fn write_retrying(writer: &mut impl io::Write, mut buffer: &[u8]) {
    while !buffer.is_empty() {
        match writer.write(buffer) {
            Ok(0) => break,
            Ok(written) => buffer = &buffer[written..],
            Err(err) if err.kind() == ErrorKind::Interrupted => {}
            Err(err) if err.kind() == ErrorKind::WouldBlock => {
                std::thread::sleep(WRITE_RETRY_DELAY);
            }
            Err(_) => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::io::{self, ErrorKind};

    use super::write_retrying;

    /// A writer that replays a scripted sequence of outcomes, then accepts everything.
    struct ScriptedWriter {
        outcomes: VecDeque<io::Result<usize>>,
        written: Vec<u8>,
    }

    impl io::Write for ScriptedWriter {
        fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
            let accepted = match self.outcomes.pop_front() {
                Some(Ok(limit)) => limit.min(buffer.len()),
                Some(Err(err)) => return Err(err),
                None => buffer.len(),
            };
            self.written.extend_from_slice(&buffer[..accepted]);
            Ok(accepted)
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn retries_on_would_block_and_partial_writes() {
        let mut writer = ScriptedWriter {
            outcomes: VecDeque::from([
                Ok(3),
                Err(ErrorKind::WouldBlock.into()),
                Err(ErrorKind::Interrupted.into()),
                Ok(2),
                Err(ErrorKind::WouldBlock.into()),
            ]),
            written: Vec::new(),
        };
        write_retrying(&mut writer, b"hello, world");
        assert_eq!(writer.written, b"hello, world");
    }

    #[test]
    fn drops_output_on_other_errors() {
        let mut writer = ScriptedWriter {
            outcomes: VecDeque::from([Err(ErrorKind::BrokenPipe.into())]),
            written: Vec::new(),
        };
        write_retrying(&mut writer, b"hello");
        assert!(writer.written.is_empty());
    }
}
