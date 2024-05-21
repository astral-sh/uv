use std::fmt::Write;

use anyhow::Result;
use itertools::Itertools;
use owo_colors::OwoColorize;
use tracing::debug;

use distribution_types::{InstalledDist, Name};
use uv_cache::Cache;
use uv_fs::Simplified;
use uv_installer::SitePackages;
use uv_interpreter::{PythonEnvironment, SystemPython};

use crate::commands::ExitStatus;
use crate::printer::Printer;

/// Enumerate the installed packages in the current environment.
pub(crate) fn pip_freeze(
    exclude_editable: bool,
    strict: bool,
    python: Option<&str>,
    system: bool,
    cache: &Cache,
    printer: Printer,
) -> Result<ExitStatus> {
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
    for dist in site_packages
        .iter()
        .filter(|dist| !(exclude_editable && dist.is_editable()))
        .sorted_unstable_by(|a, b| a.name().cmp(b.name()).then(a.version().cmp(b.version())))
    {
        match dist {
            InstalledDist::Registry(dist) => {
                writeln!(printer.stdout(), "{}=={}", dist.name().bold(), dist.version)?;
            }
            InstalledDist::Url(dist) => {
                if dist.editable {
                    writeln!(printer.stdout(), "-e {}", dist.url)?;
                } else {
                    writeln!(printer.stdout(), "{} @ {}", dist.name().bold(), dist.url)?;
                }
            }
            InstalledDist::EggInfo(dist) => {
                writeln!(printer.stdout(), "{}=={}", dist.name().bold(), dist.version)?;
            }
            InstalledDist::LegacyEditable(dist) => {
                writeln!(printer.stdout(), "-e {}", dist.target.display())?;
            }
        }
    }

    // Validate that the environment is consistent.
    if strict {
        for diagnostic in site_packages.diagnostics()? {
            writeln!(
                printer.stderr(),
                "{}{} {}",
                "warning".yellow().bold(),
                ":".bold(),
                diagnostic.message().bold()
            )?;
        }
    }

    Ok(ExitStatus::Success)
}
