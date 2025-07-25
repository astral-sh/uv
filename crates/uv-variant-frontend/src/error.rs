use std::env;
use std::fmt::{Display, Formatter};
use std::io;
use std::path::PathBuf;
use std::process::ExitStatus;

use owo_colors::OwoColorize;
use thiserror::Error;
use tracing::error;

use uv_configuration::BuildOutput;
use uv_types::AnyErrorBuild;

use crate::PythonRunnerOutput;

#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("Failed to resolve requirements from {0}")]
    RequirementsResolve(&'static str, #[source] AnyErrorBuild),
    #[error("Failed to install requirements from {0}")]
    RequirementsInstall(&'static str, #[source] AnyErrorBuild),
    #[error("Failed to create temporary virtualenv")]
    Virtualenv(#[from] uv_virtualenv::Error),
    #[error("Failed to run `{0}`")]
    CommandFailed(PathBuf, #[source] io::Error),
    #[error("The build backend returned an error")]
    ProviderBackend(#[from] ProviderBackendError),
    #[error("Failed to build PATH for build script")]
    BuildScriptPath(#[source] env::JoinPathsError),
    #[error(transparent)]
    SerdeJson(#[from] serde_json::Error),
}

#[derive(Debug, Error)]
pub struct ProviderBackendError {
    message: String,
    exit_code: ExitStatus,
    stdout: Vec<String>,
    stderr: Vec<String>,
}

impl Display for ProviderBackendError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.message, self.exit_code)?;

        let mut non_empty = false;

        if self.stdout.iter().any(|line| !line.trim().is_empty()) {
            write!(f, "\n\n{}\n{}", "[stdout]".red(), self.stdout.join("\n"))?;
            non_empty = true;
        }

        if self.stderr.iter().any(|line| !line.trim().is_empty()) {
            write!(f, "\n\n{}\n{}", "[stderr]".red(), self.stderr.join("\n"))?;
            non_empty = true;
        }

        if non_empty {
            writeln!(f)?;
        }

        write!(
            f,
            "\n{}{} This usually indicates a problem with the package or the build environment.",
            "hint".bold().cyan(),
            ":".bold()
        )?;

        Ok(())
    }
}

impl Error {
    /// Construct an [`Error`] from the output of a failed command.
    pub(crate) fn from_command_output(
        message: String,
        output: &PythonRunnerOutput,
        level: BuildOutput,
    ) -> Self {
        match level {
            BuildOutput::Stderr | BuildOutput::Quiet => {
                Self::ProviderBackend(ProviderBackendError {
                    message,
                    exit_code: output.status,
                    stdout: vec![],
                    stderr: vec![],
                })
            }
            BuildOutput::Debug => Self::ProviderBackend(ProviderBackendError {
                message,
                exit_code: output.status,
                stdout: output.stdout.clone(),
                stderr: output.stderr.clone(),
            }),
        }
    }
}
