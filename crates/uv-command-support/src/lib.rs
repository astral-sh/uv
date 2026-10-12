//! Output and process primitives shared by uv command implementations.

use std::borrow::Cow;
use std::io::stdout;
use std::path::Path;
use std::process::ExitCode;
use std::time::Duration;

use anstream::AutoStream;

pub mod child;
mod printer;
pub mod progress;
pub mod update_shell;

pub use printer::{Printer, Stderr, Stdout};

/// The process status for a command that completed without a final error to render.
#[derive(Copy, Clone)]
pub enum ExitStatus {
    /// The command succeeded.
    Success,

    /// The command reported a failure caused by user input.
    Failure,

    /// The command reported an unexpected failure.
    Error,

    /// The command's exit status is propagated from an external command.
    External(u8),
}

/// A command error propagated to the entrypoint for exit-status selection.
#[derive(Debug, thiserror::Error)]
pub enum UvError {
    /// An error caused by invalid or unsatisfiable user input.
    #[error(transparent)]
    User(anyhow::Error),

    /// An error caused by invalid command-line arguments.
    #[error(transparent)]
    Argument(anyhow::Error),

    /// An unexpected internal or environmental error.
    #[error(transparent)]
    Unexpected(anyhow::Error),
}

impl UvError {
    /// Create a user-facing error.
    pub fn user(error: impl Into<anyhow::Error>) -> Self {
        Self::User(error.into())
    }

    /// Create an argument error.
    pub fn argument(error: anyhow::Error) -> Self {
        Self::Argument(error)
    }

    /// Create an unexpected error.
    pub fn unexpected(error: anyhow::Error) -> Self {
        Self::Unexpected(error)
    }

    /// Add command-specific context to a user error without changing unexpected errors.
    #[must_use]
    pub fn map_user(self, context: impl FnOnce(anyhow::Error) -> anyhow::Error) -> Self {
        match self {
            Self::User(error) => Self::User(context(error)),
            Self::Argument(error) => Self::Argument(error),
            Self::Unexpected(error) => Self::Unexpected(error),
        }
    }
}

impl From<ExitStatus> for ExitCode {
    fn from(status: ExitStatus) -> Self {
        match status {
            ExitStatus::Success => Self::from(0),
            ExitStatus::Failure => Self::from(1),
            ExitStatus::Error => Self::from(2),
            ExitStatus::External(code) => Self::from(code),
        }
    }
}

/// Format a duration as a human-readable string, Cargo-style.
pub fn elapsed(duration: Duration) -> String {
    let secs = duration.as_secs();
    let ms = duration.subsec_millis();

    if secs >= 60 {
        format!("{}m {:02}s", secs / 60, secs % 60)
    } else if secs > 0 {
        format!("{}.{:02}s", secs, duration.subsec_nanos() / 10_000_000)
    } else if ms > 0 {
        format!("{ms}ms")
    } else {
        format!("0.{:02}ms", duration.subsec_nanos() / 10_000)
    }
}

/// A multicasting writer that writes to both the standard output and an output file, if present.
pub struct OutputWriter<'a> {
    stdout: Option<AutoStream<std::io::Stdout>>,
    output_file: Option<&'a Path>,
    buffer: Vec<u8>,
}

impl<'a> OutputWriter<'a> {
    /// Create a new output writer.
    pub fn new(include_stdout: bool, output_file: Option<&'a Path>) -> Self {
        let stdout = include_stdout.then(|| AutoStream::<std::io::Stdout>::auto(stdout()));
        Self {
            stdout,
            output_file,
            buffer: Vec::new(),
        }
    }

    /// Commit the buffer to the output file.
    pub async fn commit(self) -> std::io::Result<()> {
        if let Some(output_file) = self.output_file {
            if let Some(parent_dir) = output_file.parent() {
                fs_err::create_dir_all(parent_dir)?;
            }

            // If the output file is an existing symlink, write to the destination instead.
            let output_file = fs_err::read_link(output_file)
                .map(Cow::Owned)
                .unwrap_or(Cow::Borrowed(output_file));
            let stream = anstream::adapter::strip_bytes(&self.buffer).into_vec();
            uv_fs::write_atomic(output_file, &stream).await?;
        }
        Ok(())
    }
}

impl std::io::Write for OutputWriter<'_> {
    /// Write to both standard output and the output buffer, if present.
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        // Write to the buffer.
        if self.output_file.is_some() {
            self.buffer.write_all(buf)?;
        }

        // Write to standard output.
        if let Some(stdout) = &mut self.stdout {
            stdout.write_all(buf)?;
        }

        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        if let Some(stdout) = &mut self.stdout {
            stdout.flush()?;
        }
        Ok(())
    }
}

/// Given a list of names, return a conjunction of the names (e.g., "Alice, Bob, and Charlie").
pub fn conjunction(names: Vec<String>) -> String {
    let mut names = names.into_iter();
    let first = names.next();
    let last = names.next_back();
    match (first, last) {
        (Some(first), Some(last)) => {
            let mut result = first;
            let mut comma = false;
            for name in names {
                result.push_str(", ");
                result.push_str(&name);
                comma = true;
            }
            if comma {
                result.push_str(", and ");
            } else {
                result.push_str(" and ");
            }
            result.push_str(&last);
            result
        }
        (Some(first), None) => first,
        _ => String::new(),
    }
}
