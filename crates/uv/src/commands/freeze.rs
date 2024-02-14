use std::fmt::Write;

use anstream::println;
use anyhow::Result;
use itertools::Itertools;
use owo_colors::OwoColorize;
use tracing::debug;

use distribution_types::Name;
use platform_host::Platform;
use uv_cache::Cache;
use uv_fs::Normalized;
use uv_installer::SitePackages;
use uv_interpreter::Virtualenv;

use crate::commands::ExitStatus;
use crate::printer::Printer;

/// Enumerate the installed packages in the current environment.
pub(crate) fn freeze(cache: &Cache, strict: bool, mut printer: Printer) -> Result<ExitStatus> {
    // Detect the current Python interpreter.
    let platform = Platform::current()?;
    let venv = Virtualenv::from_env(platform, cache)?;

    debug!(
        "Using Python {} environment at {}",
        venv.interpreter().python_version(),
        venv.python_executable().normalized_display().cyan()
    );

    // Build the installed index.
    let site_packages = SitePackages::from_executable(&venv)?;
    for dist in site_packages
        .iter()
        .sorted_unstable_by(|a, b| a.name().cmp(b.name()))
    {
        println!("{dist}");
    }

    // Validate that the environment is consistent.
    if strict {
        for diagnostic in site_packages.diagnostics()? {
            writeln!(
                printer,
                "{}{} {}",
                "warning".yellow().bold(),
                ":".bold(),
                diagnostic.message().bold()
            )?;
        }
    }

    Ok(ExitStatus::Success)
}
