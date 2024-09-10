use anstream::AutoStream;
use anyhow::Context;
use owo_colors::OwoColorize;
use std::borrow::Cow;
use std::io::stdout;
use std::path::Path;
use std::time::Duration;
use std::{fmt::Display, fmt::Write, process::ExitCode};

pub(crate) use build::build;
pub(crate) use cache_clean::cache_clean;
pub(crate) use cache_dir::cache_dir;
pub(crate) use cache_prune::cache_prune;
use distribution_types::{IndexCapabilities, InstalledMetadata};
pub(crate) use help::help;
pub(crate) use pip::check::pip_check;
pub(crate) use pip::compile::pip_compile;
pub(crate) use pip::freeze::pip_freeze;
pub(crate) use pip::install::pip_install;
pub(crate) use pip::list::pip_list;
pub(crate) use pip::show::pip_show;
pub(crate) use pip::sync::pip_sync;
pub(crate) use pip::tree::pip_tree;
pub(crate) use pip::uninstall::pip_uninstall;
pub(crate) use project::add::add;
pub(crate) use project::export::export;
pub(crate) use project::init::{init, InitProjectKind};
pub(crate) use project::lock::lock;
pub(crate) use project::remove::remove;
pub(crate) use project::run::{run, RunCommand};
pub(crate) use project::sync::sync;
pub(crate) use project::tree::tree;
pub(crate) use python::dir::dir as python_dir;
pub(crate) use python::find::find as python_find;
pub(crate) use python::install::install as python_install;
pub(crate) use python::list::list as python_list;
pub(crate) use python::pin::pin as python_pin;
pub(crate) use python::uninstall::uninstall as python_uninstall;
#[cfg(feature = "self-update")]
pub(crate) use self_update::self_update;
pub(crate) use tool::dir::dir as tool_dir;
pub(crate) use tool::install::install as tool_install;
pub(crate) use tool::list::list as tool_list;
pub(crate) use tool::run::run as tool_run;
pub(crate) use tool::run::ToolRunCommand;
pub(crate) use tool::uninstall::uninstall as tool_uninstall;
pub(crate) use tool::update_shell::update_shell as tool_update_shell;
pub(crate) use tool::upgrade::upgrade as tool_upgrade;
use uv_cache::Cache;
use uv_fs::Simplified;
use uv_git::GitResolver;
use uv_installer::compile_tree;
use uv_normalize::PackageName;
use uv_python::PythonEnvironment;
use uv_resolver::InMemoryIndex;
use uv_types::InFlight;
pub(crate) use venv::venv;
pub(crate) use version::version;

use crate::printer::Printer;

mod cache_clean;
mod cache_dir;
mod cache_prune;
mod help;
pub(crate) mod pip;
mod project;
mod python;
pub(crate) mod reporters;
mod tool;

mod build;
#[cfg(feature = "self-update")]
mod self_update;
mod venv;
mod version;

#[derive(Copy, Clone)]
pub(crate) enum ExitStatus {
    /// The command succeeded.
    Success,

    /// The command failed due to an error in the user input.
    Failure,

    /// The command failed with an unexpected error.
    Error,

    /// The command's exit status is propagated from an external command.
    External(u8),
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
pub(super) fn elapsed(duration: Duration) -> String {
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

#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub(super) enum ChangeEventKind {
    /// The package was removed from the environment.
    Removed,
    /// The package was added to the environment.
    Added,
    /// The package was reinstalled without changing versions.
    Reinstalled,
}

#[derive(Debug)]
pub(super) struct ChangeEvent<'a, T: InstalledMetadata> {
    dist: &'a T,
    kind: ChangeEventKind,
}

#[derive(Debug)]
pub(super) struct DryRunEvent<T: Display> {
    name: PackageName,
    version: T,
    kind: ChangeEventKind,
}

/// Compile all Python source files in site-packages to bytecode, to speed up the
/// initial run of any subsequent executions.
///
/// See the `--compile` option on `pip sync` and `pip install`.
pub(super) async fn compile_bytecode(
    venv: &PythonEnvironment,
    cache: &Cache,
    printer: Printer,
) -> anyhow::Result<()> {
    let start = std::time::Instant::now();
    let mut files = 0;
    for site_packages in venv.site_packages() {
        files += compile_tree(&site_packages, venv.python_executable(), cache.root())
            .await
            .with_context(|| {
                format!(
                    "Failed to bytecode-compile Python file in: {}",
                    site_packages.user_display()
                )
            })?;
    }
    let s = if files == 1 { "" } else { "s" };
    writeln!(
        printer.stderr(),
        "{}",
        format!(
            "Bytecode compiled {} {}",
            format!("{files} file{s}").bold(),
            format!("in {}", elapsed(start.elapsed())).dimmed()
        )
        .dimmed()
    )?;
    Ok(())
}

/// Formats a number of bytes into a human readable SI-prefixed size.
///
/// Returns a tuple of `(quantity, units)`.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]
pub(super) fn human_readable_bytes(bytes: u64) -> (f32, &'static str) {
    static UNITS: [&str; 7] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB", "EiB"];
    let bytes = bytes as f32;
    let i = ((bytes.log2() / 10.0) as usize).min(UNITS.len() - 1);
    (bytes / 1024_f32.powi(i as i32), UNITS[i])
}

/// Shared state used during resolution and installation.
#[derive(Default)]
pub(crate) struct SharedState {
    /// The resolved Git references.
    pub(crate) git: GitResolver,
    /// The fetched package versions and metadata.
    pub(crate) index: InMemoryIndex,
    /// The downloaded distributions.
    pub(crate) in_flight: InFlight,
    /// The discovered capabilities for each registry index.
    pub(crate) capabilities: IndexCapabilities,
}

/// A multicasting writer that writes to both the standard output and an output file, if present.
#[allow(clippy::disallowed_types)]
struct OutputWriter<'a> {
    stdout: Option<AutoStream<std::io::Stdout>>,
    output_file: Option<&'a Path>,
    buffer: Vec<u8>,
}

#[allow(clippy::disallowed_types)]
impl<'a> OutputWriter<'a> {
    /// Create a new output writer.
    fn new(include_stdout: bool, output_file: Option<&'a Path>) -> Self {
        let stdout = include_stdout.then(|| AutoStream::<std::io::Stdout>::auto(stdout()));
        Self {
            stdout,
            output_file,
            buffer: Vec::new(),
        }
    }

    /// Write the given arguments to both standard output and the output buffer, if present.
    fn write_fmt(&mut self, args: std::fmt::Arguments<'_>) -> std::io::Result<()> {
        use std::io::Write;

        // Write to the buffer.
        if self.output_file.is_some() {
            self.buffer.write_fmt(args)?;
        }

        // Write to standard output.
        if let Some(stdout) = &mut self.stdout {
            write!(stdout, "{args}")?;
        }

        Ok(())
    }

    /// Commit the buffer to the output file.
    async fn commit(self) -> std::io::Result<()> {
        if let Some(output_file) = self.output_file {
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
