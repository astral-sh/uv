use std::fmt::Write;

use anyhow::Context;
use owo_colors::OwoColorize;

use uv_fs::Simplified;
use uv_python::managed::{ManagedPythonInstallations, python_executable_dir};

use crate::printer::Printer;

/// Show the Python installation directory.
pub(crate) fn dir(bin: bool, printer: Printer) -> anyhow::Result<()> {
    if bin {
        let bin = python_executable_dir()?;
        writeln!(printer.stdout(), "{}", bin.simplified_display().cyan())?;
    } else {
        let installed_toolchains = ManagedPythonInstallations::from_settings(None)
            .context("Failed to initialize toolchain settings")?;
        writeln!(
            printer.stdout(),
            "{}",
            installed_toolchains.root().simplified_display().cyan()
        )?;
    }

    Ok(())
}
