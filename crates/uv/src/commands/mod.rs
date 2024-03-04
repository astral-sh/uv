use std::fmt::Write;
use std::process::ExitCode;
use std::time::Duration;

use anyhow::Context;
use owo_colors::OwoColorize;

pub(crate) use cache_clean::cache_clean;
pub(crate) use cache_dir::cache_dir;
use distribution_types::InstalledMetadata;
pub(crate) use pip_compile::{extra_name_with_clap_error, pip_compile, Upgrade};
pub(crate) use pip_freeze::pip_freeze;
pub(crate) use pip_install::pip_install;
pub(crate) use pip_list::pip_list;
pub(crate) use pip_sync::pip_sync;
pub(crate) use pip_uninstall::pip_uninstall;
use uv_installer::compile_tree;
use uv_interpreter::PythonEnvironment;
pub(crate) use venv::venv;
pub(crate) use version::version;

use crate::printer::Printer;

mod cache_clean;
mod cache_dir;
mod pip_compile;
mod pip_freeze;
mod pip_install;
mod pip_list;
mod pip_sync;
mod pip_uninstall;
mod reporters;
mod venv;
mod version;

#[derive(Copy, Clone)]
pub(crate) enum ExitStatus {
    /// The command succeeded.
    #[allow(unused)]
    Success,

    /// The command failed due to an error in the user input.
    #[allow(unused)]
    Failure,

    /// The command failed with an unexpected error.
    #[allow(unused)]
    Error,
}

impl From<ExitStatus> for ExitCode {
    fn from(status: ExitStatus) -> Self {
        match status {
            ExitStatus::Success => Self::from(0),
            ExitStatus::Failure => Self::from(1),
            ExitStatus::Error => Self::from(2),
        }
    }
}

/// Format a duration as a human-readable string, Cargo-style.
pub(super) fn elapsed(duration: Duration) -> String {
    let secs = duration.as_secs();

    if secs >= 60 {
        format!("{}m {:02}s", secs / 60, secs % 60)
    } else if secs > 0 {
        format!("{}.{:02}s", secs, duration.subsec_nanos() / 10_000_000)
    } else {
        format!("{}ms", duration.subsec_millis())
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub(super) enum ChangeEventKind {
    /// The package was removed from the environment.
    Removed,
    /// The package was added to the environment.
    Added,
}

#[derive(Debug)]
pub(super) struct ChangeEvent<T: InstalledMetadata> {
    dist: T,
    kind: ChangeEventKind,
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub(crate) enum VersionFormat {
    Text,
    Json,
}

/// Compile all Python source files in the site-packages of the venv to bytecode, to speed up the
/// first run. See the `--compile` option on `pip sync` and `pip install`.
pub(super) async fn compile_venv(
    printer: &mut Printer,
    venv: &PythonEnvironment,
) -> anyhow::Result<()> {
    let start = std::time::Instant::now();
    let files = compile_tree(venv.site_packages(), venv.python_executable())
        .await
        .with_context(|| {
            format!(
                "Failed to bytecode compile {}",
                venv.site_packages().display()
            )
        })?;
    let s = if files == 1 { "" } else { "s" };
    writeln!(
        printer,
        "{}",
        format!(
            "Bytecode compiled {} in {}",
            format!("{files} file{s}").bold(),
            elapsed(start.elapsed())
        )
        .dimmed()
    )?;
    Ok(())
}
