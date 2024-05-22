use std::fmt::Write;

use anyhow::Result;
use distribution_types::InstalledDist;
use owo_colors::OwoColorize;
use std::time::Instant;
use tracing::debug;

use uv_cache::Cache;
use uv_fs::Simplified;
use uv_installer::{Diagnostic, SitePackages};
use uv_interpreter::{PythonEnvironment, SystemPython};

use crate::commands::{elapsed, ExitStatus};
use crate::printer::Printer;

/// Check for incompatibilities in installed packages.
pub(crate) fn pip_check(
    python: Option<&str>,
    system: bool,
    cache: &Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    let start = Instant::now();

    // Detect the current Python interpreter.
    let system = if system {
        SystemPython::Required
    } else {
        SystemPython::Allowed
    };
    let venv = PythonEnvironment::find(python, system, cache)?;

    debug!(
        "Using Python {} environment at {}",
        venv.interpreter().python_version(),
        venv.python_executable().user_display().cyan()
    );

    // Build the installed index.
    let site_packages = SitePackages::from_executable(&venv)?;
    let packages: Vec<&InstalledDist> = site_packages.iter().collect();

    let s = if packages.len() == 1 { "" } else { "s" };
    writeln!(
        printer.stderr(),
        "{}",
        format!(
            "Checked {} in {}",
            format!("{} package{}", packages.len(), s).bold(),
            elapsed(start.elapsed())
        )
        .dimmed()
    )?;

    let diagnostics: Vec<Diagnostic> = site_packages.diagnostics()?.into_iter().collect();

    if diagnostics.is_empty() {
        writeln!(
            printer.stderr(),
            "{}",
            "All installed packages are compatible".to_string().dimmed()
        )?;

        Ok(ExitStatus::Success)
    } else {
        let incompats = if diagnostics.len() == 1 {
            "incompatibility"
        } else {
            "incompatibilities"
        };
        writeln!(
            printer.stderr(),
            "{}",
            format!(
                "Found {}",
                format!("{} {}", diagnostics.len(), incompats).bold()
            )
            .dimmed()
        )?;

        for diagnostic in &diagnostics {
            writeln!(printer.stderr(), "{}", diagnostic.message().bold())?;
        }

        Ok(ExitStatus::Failure)
    }
}
