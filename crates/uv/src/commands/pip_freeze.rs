use std::fmt::Write;

use anyhow::Result;
use itertools::Itertools;
use owo_colors::OwoColorize;
use tracing::debug;

use distribution_types::{InstalledDist, Name};
use platform_host::Platform;
use uv_cache::Cache;
use uv_fs::Simplified;
use uv_installer::SitePackages;
use uv_interpreter::PythonEnvironment;

use crate::commands::ExitStatus;
use crate::printer::Printer;

/// Enumerate the installed packages in the current environment.
pub(crate) fn pip_freeze(
    strict: bool,
    python: Option<&str>,
    system: bool,
    cache: &Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    // Detect the current Python interpreter.
    let platform = Platform::current()?;
    let venv = if let Some(python) = python {
        PythonEnvironment::from_requested_python(python, &platform, cache)?
    } else if system {
        PythonEnvironment::from_default_python(&platform, cache)?
    } else {
        match PythonEnvironment::from_virtualenv(platform.clone(), cache) {
            Ok(venv) => venv,
            Err(uv_interpreter::Error::VenvNotFound) => {
                PythonEnvironment::from_default_python(&platform, cache)?
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
    for dist in site_packages
        .iter()
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
