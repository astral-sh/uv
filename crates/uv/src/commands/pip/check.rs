use std::fmt::Write;
use std::time::Instant;

use anyhow::Result;
use owo_colors::OwoColorize;
use tracing::debug;

use distribution_types::{Diagnostic, InstalledDist};
use uv_cache::Cache;
use uv_fs::Simplified;
use uv_installer::{SitePackages, SitePackagesDiagnostic};
use uv_python::{EnvironmentPreference, PythonEnvironment, PythonRequest};

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
    let environment = PythonEnvironment::find(
        &python.map(PythonRequest::parse).unwrap_or_default(),
        EnvironmentPreference::from_system_flag(system, false),
        cache,
    )?;

    debug!(
        "Using Python {} environment at {}",
        environment.interpreter().python_version(),
        environment.python_executable().user_display().cyan()
    );

    // Build the installed index.
    let site_packages = SitePackages::from_environment(&environment)?;
    let packages: Vec<&InstalledDist> = site_packages.iter().collect();

    let s = if packages.len() == 1 { "" } else { "s" };
    writeln!(
        printer.stderr(),
        "{}",
        format!(
            "Checked {} {}",
            format!("{} package{}", packages.len(), s).bold(),
            format!("in {}", elapsed(start.elapsed())).dimmed()
        )
        .dimmed()
    )?;

    // Determine the markers to use for resolution.
    let markers = environment.interpreter().resolver_markers();

    // Run the diagnostics.
    let diagnostics: Vec<SitePackagesDiagnostic> =
        site_packages.diagnostics(&markers)?.into_iter().collect();

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
