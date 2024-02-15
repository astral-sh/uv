use std::process::ExitCode;
use std::time::Duration;

pub(crate) use clean::clean;
use distribution_types::InstalledMetadata;
pub(crate) use freeze::freeze;
pub(crate) use pip_compile::{extra_name_with_clap_error, pip_compile, Upgrade};
pub(crate) use pip_install::pip_install;
pub(crate) use pip_sync::pip_sync;
pub(crate) use pip_uninstall::pip_uninstall;
pub(crate) use venv::venv;

mod clean;
mod freeze;
mod pip_compile;
mod pip_install;
mod pip_sync;
mod pip_uninstall;
mod reporters;
mod venv;

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
            ExitStatus::Success => ExitCode::from(0),
            ExitStatus::Failure => ExitCode::from(1),
            ExitStatus::Error => ExitCode::from(2),
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
