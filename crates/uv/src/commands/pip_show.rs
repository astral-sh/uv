use std::collections::BTreeSet;
use std::fmt::Write;

use anyhow::Result;
use itertools::Itertools;
use owo_colors::OwoColorize;
use tracing::debug;

use distribution_types::Name;
use platform_host::Platform;
use uv_cache::Cache;
use uv_fs::Simplified;
use uv_installer::SitePackages;
use uv_interpreter::PythonEnvironment;
use uv_normalize::PackageName;

use crate::commands::ExitStatus;
use crate::printer::Printer;

/// Show information about one or more installed packages.
pub(crate) fn pip_show(
    mut packages: Vec<PackageName>,
    strict: bool,
    python: Option<&str>,
    system: bool,
    cache: &Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    if packages.is_empty() {
        #[allow(clippy::print_stderr)]
        {
            writeln!(
                printer.stderr(),
                "{}{} Please provide a package name or names.",
                "warning".yellow().bold(),
                ":".bold(),
            )?;
        }
        return Ok(ExitStatus::Failure);
    }

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

    // Determine the markers to use for resolution.
    let markers = venv.interpreter().markers();

    // Sort and deduplicate the packages, which are keyed by name.
    packages.sort_unstable();
    packages.dedup();

    // Map to the local distributions.
    let distributions = {
        let mut distributions = Vec::with_capacity(packages.len());

        // Identify all packages that are installed.
        for package in &packages {
            let installed = site_packages.get_packages(package);
            if installed.is_empty() {
                writeln!(
                    printer.stderr(),
                    "{}{} Package(s) not found for: {}",
                    "warning".yellow().bold(),
                    ":".bold(),
                    package.as_ref().bold()
                )?;
            } else {
                distributions.extend(installed);
            }
        }

        distributions
    };

    // Like `pip`, if no packages were found, return a failure.
    if distributions.is_empty() {
        return Ok(ExitStatus::Failure);
    }

    // Print the information for each package.
    let mut first = true;
    for distribution in &distributions {
        if first {
            first = false;
        } else {
            // Print a separator between packages.
            writeln!(printer.stdout(), "---")?;
        }

        // Print the name, version, and location (e.g., the `site-packages` directory).
        writeln!(printer.stdout(), "Name: {}", distribution.name())?;
        writeln!(printer.stdout(), "Version: {}", distribution.version())?;
        writeln!(
            printer.stdout(),
            "Location: {}",
            distribution
                .path()
                .parent()
                .expect("package path is not root")
                .simplified_display()
        )?;

        // If available, print the requirements.
        if let Ok(metadata) = distribution.metadata() {
            let requires_dist = metadata
                .requires_dist
                .into_iter()
                .filter(|req| req.evaluate_markers(markers, &[]))
                .map(|req| req.name)
                .collect::<BTreeSet<_>>();
            if requires_dist.is_empty() {
                writeln!(printer.stdout(), "Requires:")?;
            } else {
                writeln!(
                    printer.stdout(),
                    "Requires: {}",
                    requires_dist.into_iter().join(", ")
                )?;
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
