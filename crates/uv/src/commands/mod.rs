use std::time::Duration;
use std::{fmt::Display, fmt::Write, process::ExitCode};

use anyhow::Context;
use owo_colors::OwoColorize;

pub(crate) use cache_clean::cache_clean;
pub(crate) use cache_dir::cache_dir;
pub(crate) use cache_prune::cache_prune;
use distribution_types::InstalledMetadata;
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
pub(crate) use project::lock::lock;
pub(crate) use project::remove::remove;
pub(crate) use project::run::run;
pub(crate) use project::sync::sync;
#[cfg(feature = "self-update")]
pub(crate) use self_update::self_update;
pub(crate) use tool::install::install as tool_install;
pub(crate) use tool::run::run as run_tool;
pub(crate) use toolchain::find::find as toolchain_find;
pub(crate) use toolchain::install::install as toolchain_install;
pub(crate) use toolchain::list::list as toolchain_list;
use uv_cache::Cache;
use uv_fs::Simplified;
use uv_installer::compile_tree;
use uv_normalize::PackageName;
use uv_toolchain::PythonEnvironment;
pub(crate) use venv::venv;
pub(crate) use version::version;

use crate::printer::Printer;

mod cache_clean;
mod cache_dir;
mod cache_prune;
pub(crate) mod pip;
mod project;
pub(crate) mod reporters;
mod tool;
mod toolchain;

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
}

#[derive(Debug)]
pub(super) struct ChangeEvent<T: InstalledMetadata> {
    dist: T,
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
            "Bytecode compiled {} in {}",
            format!("{files} file{s}").bold(),
            elapsed(start.elapsed())
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
