use std::fmt::Write;
use std::path::Path;

use anyhow::Result;
use fs_err::tokio as fs;
use owo_colors::OwoColorize;

use crate::commands::ExitStatus;
use crate::printer::Printer;

/// Create a virtual environment.
pub(crate) async fn venv(
    path: &Path,
    base_python: Option<&Path>,
    mut printer: Printer,
) -> Result<ExitStatus> {
    // Locate the Python interpreter.
    // TODO(charlie): Look at how Maturin discovers and ranks all the available Python interpreters.
    let base_python = if let Some(base_python) = base_python {
        base_python.to_path_buf()
    } else {
        which::which("python3").or_else(|_| which::which("python"))?
    };
    let interpreter_info = gourgeist::get_interpreter_info(&base_python)?;
    writeln!(
        printer,
        "Using Python interpreter: {}",
        base_python.display().cyan()
    )?;

    // If the path already exists, remove it.
    fs::remove_file(path).await.ok();
    fs::remove_dir_all(path).await.ok();

    writeln!(
        printer,
        "Creating virtual environment at: {}",
        path.display().cyan()
    )?;

    // Create the virtual environment.
    gourgeist::create_venv(path, &base_python, &interpreter_info, true)?;

    Ok(ExitStatus::Success)
}
