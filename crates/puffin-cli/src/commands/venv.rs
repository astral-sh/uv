use std::fmt::Write;
use std::path::{Path, PathBuf};

use anyhow::Result;
use fs_err as fs;
use miette::{Diagnostic, IntoDiagnostic};
use owo_colors::OwoColorize;
use thiserror::Error;

use platform_host::Platform;
use puffin_cache::Cache;
use puffin_interpreter::Interpreter;

use crate::commands::ExitStatus;
use crate::printer::Printer;

/// Create a virtual environment.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn venv(
    path: &Path,
    base_python: Option<&Path>,
    cache: &Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    match venv_impl(path, base_python, cache, printer) {
        Ok(status) => Ok(status),
        Err(err) => {
            #[allow(clippy::print_stderr)]
            {
                eprint!("{err:?}");
            }
            Ok(ExitStatus::Failure)
        }
    }
}

#[derive(Error, Debug, Diagnostic)]
enum VenvError {
    #[error("Unable to find a Python interpreter")]
    #[diagnostic(code(puffin::venv::python_not_found))]
    PythonNotFound,

    #[error("Unable to find a Python interpreter {0}")]
    #[diagnostic(code(puffin::venv::python_not_found))]
    UserPythonNotFound(PathBuf),

    #[error("Failed to extract Python interpreter info")]
    #[diagnostic(code(puffin::venv::interpreter))]
    InterpreterError(#[source] puffin_interpreter::Error),

    #[error("Failed to create virtual environment")]
    #[diagnostic(code(puffin::venv::creation))]
    CreationError(#[source] gourgeist::Error),
}

/// Create a virtual environment.
fn venv_impl(
    path: &Path,
    base_python: Option<&Path>,
    cache: &Cache,
    mut printer: Printer,
) -> miette::Result<ExitStatus> {
    // Locate the Python interpreter.
    let base_python = if let Some(base_python) = base_python {
        fs::canonicalize(
            which::which_global(base_python)
                .map_err(|_| VenvError::UserPythonNotFound(base_python.to_path_buf()))?,
        )
        .into_diagnostic()?
    } else {
        fs::canonicalize(
            which::which_global("python3")
                .or_else(|_| which::which_global("python"))
                .map_err(|_| VenvError::PythonNotFound)?,
        )
        .into_diagnostic()?
    };

    let platform = Platform::current().into_diagnostic()?;
    let interpreter =
        Interpreter::query(&base_python, platform, cache).map_err(VenvError::InterpreterError)?;

    writeln!(
        printer,
        "Using Python {} at {}",
        interpreter.version(),
        interpreter.sys_executable().display().cyan()
    )
    .into_diagnostic()?;

    writeln!(
        printer,
        "Creating virtual environment at: {}",
        path.display().cyan()
    )
    .into_diagnostic()?;

    // Create the virtual environment.
    gourgeist::create_venv(path, interpreter).map_err(VenvError::CreationError)?;

    Ok(ExitStatus::Success)
}
