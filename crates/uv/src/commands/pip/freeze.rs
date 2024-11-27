use std::fmt::Write;

use anyhow::Result;
use itertools::Itertools;
use owo_colors::OwoColorize;

use uv_cache::Cache;
use uv_distribution_types::{Diagnostic, InstalledDist, Name};
use uv_installer::SitePackages;
use uv_python::{EnvironmentPreference, PythonEnvironment, PythonRequest};

use crate::commands::pip::operations::report_target_environment;
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
    let environment = PythonEnvironment::find(
        &python.map(PythonRequest::parse).unwrap_or_default(),
        EnvironmentPreference::from_system_flag(system, false),
        cache,
    )?;

    report_target_environment(&environment, cache, printer)?;

    // Build the installed index.
    let site_packages = SitePackages::from_environment(&environment)?;
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
            InstalledDist::EggInfoFile(dist) => {
                writeln!(printer.stdout(), "{}=={}", dist.name().bold(), dist.version)?;
            }
            InstalledDist::EggInfoDirectory(dist) => {
                writeln!(printer.stdout(), "{}=={}", dist.name().bold(), dist.version)?;
            }
            InstalledDist::LegacyEditable(dist) => {
                writeln!(printer.stdout(), "-e {}", dist.target.display())?;
            }
        }
    }

    // Validate that the environment is consistent.
    if strict {
        // Determine the markers to use for resolution.
        let markers = environment.interpreter().resolver_marker_environment();

        for diagnostic in site_packages.diagnostics(&markers)? {
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
