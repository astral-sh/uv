use std::fmt::Write;

use anyhow::Result;
use owo_colors::OwoColorize;
use tracing::debug;

use uv_cache::Cache;
use uv_fs::Simplified;
use uv_installer::SitePackages;
use uv_interpreter::PythonEnvironment;

use crate::commands::ExitStatus;
use crate::printer::Printer;

/// Show information about one or more installed packages.
pub(crate) fn pip_check(
    python: Option<&str>,
    system: bool,
    cache: &Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    // Detect the current Python interpreter.
    let venv = if let Some(python) = python {
        PythonEnvironment::from_requested_python(python, cache)?
    } else if system {
        PythonEnvironment::from_default_python(cache)?
    } else {
        match PythonEnvironment::from_virtualenv(cache) {
            Ok(venv) => venv,
            Err(uv_interpreter::Error::VenvNotFound) => {
                PythonEnvironment::from_default_python(cache)?
            }
            Err(err) => return Err(err.into()),
        }
    };

    debug!(
        "Using Python {} environment at {}",
        venv.interpreter().python_version(),
        venv.python_executable().simplified_display().cyan()
    );

    // Build the installed index.
    let site_packages = SitePackages::from_executable(&venv)?;

    let mut is_compatible = true;
    // This loop is entered if and only if there is at least one conflict.
    for diagnostic in site_packages.diagnostics()? {
        is_compatible = false;
        writeln!(printer.stdout(), "{}", diagnostic.message())?;
    }

    if !is_compatible {
        return Ok(ExitStatus::Failure);
    }

    writeln!(printer.stdout(), "Installed packages pass the check.").unwrap();
    Ok(ExitStatus::Success)
}
