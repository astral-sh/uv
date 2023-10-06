use std::process::ExitCode;

pub(crate) use compile::compile;
pub(crate) use install::install;

mod compile;
mod install;

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
