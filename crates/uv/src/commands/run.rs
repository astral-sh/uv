use std::ffi::OsString;
use std::{env, iter};

use anyhow::Result;
use owo_colors::OwoColorize;
use tempfile::{tempdir_in, TempDir};
use tracing::debug;
use uv_fs::Simplified;
use uv_interpreter::PythonEnvironment;

use crate::commands::ExitStatus;
use tokio::process::Command;
use uv_cache::Cache;

/// Run a command.
#[allow(clippy::unnecessary_wraps, clippy::too_many_arguments)]
pub(crate) async fn run(
    command: String,
    args: Vec<String>,
    isolated: bool,
    cache: &Cache,
) -> Result<ExitStatus> {
    // Detect the current Python interpreter.
    // TODO(zanieb): Create ephemeral environments
    // TODO(zanieb): Accept `--python`
    let run_env = environment_for_run(isolated, cache)?;
    let python_env = run_env.python;

    // Construct the command
    let mut process = Command::new(&command);
    process.args(&args);

    // Set up the PATH
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

    // Spawn and wait for completion
    // Standard input, output, and error streams are all inherited
    // TODO(zanieb): Throw a nicer error message if the command is not found
    debug!("Running `{command} {}`", args.join(" "));
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

struct RunEnvironment {
    /// The Python environment to execute the run in.
    python: PythonEnvironment,
    /// A temporary directory, if a new virtual environment was created.
    ///
    /// Included to ensure that the temporary directory exists for the length of the operation, but
    /// is dropped at the end as appropriate.
    _temp_dir_drop: Option<TempDir>,
}

/// Returns an environment for a `run` invocation.
///
/// Will use the current virtual environment (if any) unless `isolated` is true.
/// Will create virtual environments in a temporary directory (if necessary).
fn environment_for_run(isolated: bool, cache: &Cache) -> Result<RunEnvironment> {
    if !isolated {
        // Return the active environment if it exists
        match PythonEnvironment::from_virtualenv(cache) {
            Ok(env) => {
                return Ok(RunEnvironment {
                    python: env,
                    _temp_dir_drop: None,
                })
            }
            Err(uv_interpreter::Error::VenvNotFound) => {}
            Err(err) => return Err(err.into()),
        };
    }

    // Find an interpreter to use
    // TODO(zanieb): Populate `python` from the user
    let python = None;
    let python_env = if let Some(python) = python {
        PythonEnvironment::from_requested_python(python, cache)?
    } else {
        PythonEnvironment::from_default_python(cache)?
    };

    // Create a virtual environment directory
    // TODO(zanieb): Move this path derivation elsewhere
    let uv_state_path = std::env::current_dir()?.join(".uv");
    fs_err::create_dir_all(&uv_state_path)?;
    let tmpdir = tempdir_in(uv_state_path)?;

    // Create the environment
    // TODO(zanieb): Add dependencies to the env
    Ok(RunEnvironment {
        python: uv_virtualenv::create_venv(
            tmpdir.path(),
            python_env.into_interpreter(),
            uv_virtualenv::Prompt::None,
            false,
            Vec::new(),
        )?,
        _temp_dir_drop: Some(tmpdir),
    })
}
