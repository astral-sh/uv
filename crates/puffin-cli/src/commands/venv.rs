use std::fmt::Write;
use std::path::Path;

use anyhow::Result;
use colored::Colorize;
use fs_err as fs;
use miette::{Diagnostic, IntoDiagnostic};
use platform_host::Platform;
use puffin_interpreter::InterpreterInfo;
use thiserror::Error;

use crate::commands::ExitStatus;
use crate::printer::Printer;

/// Create a virtual environment.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn venv(
    path: &Path,
    base_python: Option<&Path>,
    printer: Printer,
) -> Result<ExitStatus> {
    match venv_impl(path, base_python, printer) {
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

    #[error("Failed to extract Python interpreter info")]
    #[diagnostic(code(puffin::venv::interpreter))]
    InterpreterError(#[source] anyhow::Error),

    #[error("Failed to create virtual environment")]
    #[diagnostic(code(puffin::venv::creation))]
    CreationError(#[source] gourgeist::Error),
}

/// Create a virtual environment.
fn venv_impl(
    path: &Path,
    base_python: Option<&Path>,
    mut printer: Printer,
) -> miette::Result<ExitStatus> {
    // Locate the Python interpreter.
    // TODO(charlie): Look at how Maturin discovers and ranks all the available Python interpreters.
    let base_python = if let Some(base_python) = base_python {
        base_python.to_path_buf()
    } else {
        which::which("python3")
            .or_else(|_| which::which("python"))
            .map_err(|_| VenvError::PythonNotFound)?
    };
    let platform = Platform::current().into_diagnostic()?;
    // TODO(konstin): Add caching
    let interpreter_info = InterpreterInfo::query_cached(&base_python, platform, None)
        .map_err(VenvError::InterpreterError)?;

    writeln!(
        printer,
        "Using Python interpreter: {}",
        format!("{}", base_python.display()).cyan()
    )
    .into_diagnostic()?;

    // If the path already exists, remove it.
    fs::remove_file(path).ok();
    fs::remove_dir_all(path).ok();

    writeln!(
        printer,
        "Creating virtual environment at: {}",
        format!("{}", path.display()).cyan()
    )
    .into_diagnostic()?;

    // Create the virtual environment.
    gourgeist::create_venv(path, &base_python, &interpreter_info, true)
        .map_err(VenvError::CreationError)?;

    Ok(ExitStatus::Success)
}
