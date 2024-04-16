use std::ffi::OsString;
use std::{env, iter};

use anyhow::Result;
use owo_colors::OwoColorize;
use tracing::debug;
use uv_fs::Simplified;
use uv_interpreter::PythonEnvironment;

use crate::commands::ExitStatus;
use tokio::process::Command;
use uv_cache::Cache;

/// Run a command.
#[allow(clippy::unnecessary_wraps, clippy::too_many_arguments)]
pub(crate) async fn run(command: String, args: Vec<String>, cache: &Cache) -> Result<ExitStatus> {
    debug!("Running `{command} {}`", args.join(" "));

    // Detect the current Python interpreter.
    // TODO(zanieb): Create ephemeral environments
    // TODO(zanieb): Accept `--python`
    let python_env = match PythonEnvironment::from_virtualenv(cache) {
        Ok(env) => Some(env),
        Err(uv_interpreter::Error::VenvNotFound) => None,
        Err(err) => return Err(err.into()),
    };

    // Construct the command
    let mut process = Command::new(command);
    process.args(args);

    // Set up the PATH
    if let Some(python_env) = python_env {
        debug!(
            "Using Python {} environment at {}",
            python_env.interpreter().python_version(),
            python_env.python_executable().user_display().cyan()
        );
        let new_path = if let Some(path) = std::env::var_os("PATH") {
            let python_env_path =
                iter::once(python_env.scripts().to_path_buf()).chain(env::split_paths(&path));
            env::join_paths(python_env_path)?
        } else {
            OsString::from(python_env.scripts())
        };

        process.env("PATH", new_path);
    };

    // Spawn and wait for completion
    // Standard input, output, and error streams are all inherited
    // TODO(zanieb): Throw a nicer error message if the command is not found
    let mut handle = process.spawn()?;
    let status = handle.wait().await?;

    // Exit based on the result of the command
    // TODO(zanieb): Do we want to exit with the code of the child process? Probably.
    if status.success() {
        Ok(ExitStatus::Success)
    } else {
        Ok(ExitStatus::Failure)
    }
}
