use std::io;
use std::path::Path;

use thiserror::Error;

use uv_python::{Interpreter, PythonEnvironment};

mod virtualenv;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("Could not find a suitable Python executable for the virtual environment based on the interpreter: {0}")]
    NotFound(String),
}

/// The value to use for the shell prompt when inside a virtual environment.
#[derive(Debug)]
pub enum Prompt {
    /// Use the current directory name as the prompt.
    CurrentDirectoryName,
    /// Use the fixed string as the prompt.
    Static(String),
    /// Default to no prompt. The prompt is then set by the activator script
    /// to the virtual environment's directory name.
    None,
}

impl Prompt {
    /// Determine the prompt value to be used from the command line arguments.
    pub fn from_args(prompt: Option<String>) -> Self {
        match prompt {
            Some(prompt) if prompt == "." => Self::CurrentDirectoryName,
            Some(prompt) => Self::Static(prompt),
            None => Self::None,
        }
    }
}

// TODO(zanieb): Consider folding `allow_existing` into this?
#[derive(Debug, Clone, Copy)]
pub enum VenvForceMode {
    /// Do not replace an existing directory.
    Disabled,
    /// Replace an existing directory, if it is a virtual environment.
    ReplaceEnvironment,
    /// Replace an existing directory, regardless of contents.
    ReplaceAny,
}

impl VenvForceMode {
    pub fn from_args(force: u8) -> Self {
        if force == 0 {
            VenvForceMode::Disabled
        } else if force == 1 {
            VenvForceMode::ReplaceEnvironment
        } else {
            VenvForceMode::ReplaceAny
        }
    }
}

/// Create a virtualenv.
#[allow(clippy::fn_params_excessive_bools)]
pub fn create_venv(
    location: &Path,
    interpreter: Interpreter,
    prompt: Prompt,
    system_site_packages: bool,
    allow_existing: bool,
    force: VenvForceMode,
    relocatable: bool,
    seed: bool,
) -> Result<PythonEnvironment, Error> {
    // Create the virtualenv at the given location.
    let virtualenv = virtualenv::create(
        location,
        &interpreter,
        prompt,
        system_site_packages,
        allow_existing,
        force,
        relocatable,
        seed,
    )?;

    // Create the corresponding `PythonEnvironment`.
    let interpreter = interpreter.with_virtualenv(virtualenv);
    Ok(PythonEnvironment::from_interpreter(interpreter))
}
