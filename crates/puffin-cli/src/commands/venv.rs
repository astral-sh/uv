use std::fmt::Write;
use std::path::Path;

use anyhow::Result;
use colored::Colorize;

use crate::commands::ExitStatus;
use crate::printer::Printer;

/// Create a virtual environment.
pub(crate) async fn venv(path: &Path, mut printer: Printer) -> Result<ExitStatus> {
    // Locate the Python interpreter.
    // TODO(charlie): Look at how Maturin discovers and ranks all the available Python interpreters.
    let executable = which::which("python3").or_else(|_| which::which("python"))?;
    let interpreter_info = gourgeist::get_interpreter_info(&executable)?;
    writeln!(
        printer,
        "Using Python interpreter: {}",
        format!("{}", executable.display()).cyan()
    )?;

    // If the path already exists, remove it.
    tokio::fs::remove_file(path).await.ok();
    tokio::fs::remove_dir_all(path).await.ok();

    writeln!(
        printer,
        "Creating virtual environment at: {}",
        format!("{}", path.display()).cyan()
    )?;

    // Create the virtual environment.
    gourgeist::create_venv(path, &executable, &interpreter_info, true)?;

    Ok(ExitStatus::Success)
}
