use std::fmt::Write;

use owo_colors::OwoColorize;
use uv_fs::Simplified;
use uv_python::PythonInstallation;

use crate::printer::Printer;

pub(crate) mod installation;
pub(crate) mod malware;
pub(crate) mod resolution;
pub(crate) mod sync;

#[derive(Debug, Clone, Copy)]
pub(crate) enum Modifications {
    /// Use `pip install` semantics, whereby existing installations are left as-is, unless they are
    /// marked for re-installation or upgrade.
    ///
    /// Ensures that the resulting environment is sufficient to meet the requirements, but without
    /// any unnecessary changes.
    Sufficient,
    /// Use `pip sync` semantics, whereby any existing, extraneous installations are removed.
    ///
    /// Ensures that the resulting environment is an exact match for the requirements, but may
    /// result in more changes than necessary.
    Exact,
}

/// Display a message about the interpreter that was selected for the operation.
pub(crate) fn report_interpreter(
    python: &PythonInstallation,
    dimmed: bool,
    printer: Printer,
) -> std::fmt::Result {
    let managed = python.source().is_managed();
    let implementation = python.implementation();
    let interpreter = python.interpreter();

    if dimmed {
        if managed {
            writeln!(
                printer.stderr(),
                "{}",
                format!(
                    "Using {} {}{}",
                    implementation.pretty(),
                    interpreter.python_version(),
                    interpreter.variant().display_suffix(),
                )
                .dimmed()
            )?;
        } else {
            writeln!(
                printer.stderr(),
                "{}",
                format!(
                    "Using {} {}{} interpreter at: {}",
                    implementation.pretty(),
                    interpreter.python_version(),
                    interpreter.variant().display_suffix(),
                    interpreter.sys_executable().user_display()
                )
                .dimmed()
            )?;
        }
    } else {
        if managed {
            writeln!(
                printer.stderr(),
                "Using {} {}{}",
                implementation.pretty(),
                interpreter.python_version().cyan(),
                interpreter.variant().display_suffix().cyan()
            )?;
        } else {
            writeln!(
                printer.stderr(),
                "Using {} {}{} interpreter at: {}",
                implementation.pretty(),
                interpreter.python_version(),
                interpreter.variant().display_suffix(),
                interpreter.sys_executable().user_display().cyan()
            )?;
        }
    }

    Ok(())
}
