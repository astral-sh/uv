use std::collections::BTreeSet;
use std::fmt::Write;

use anyhow::Result;
use itertools::{Either, Itertools};
use owo_colors::OwoColorize;
use tracing::debug;

use distribution_types::Name;
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
        venv.python_executable().user_display().cyan()
    );

    // Build the installed index.
    let site_packages = SitePackages::from_executable(&venv)?;

    // Determine the markers to use for resolution.
    let markers = venv.interpreter().markers();

    // Sort and deduplicate the packages, which are keyed by name.
    packages.sort_unstable();
    packages.dedup();

    // Map to the local distributions and collect missing packages.
    let (missing, distributions): (Vec<_>, Vec<_>) = packages.iter().partition_map(|name| {
        let installed = site_packages.get_packages(name);
        if installed.is_empty() {
            Either::Left(name)
        } else {
            Either::Right(installed)
        }
    });

    if !missing.is_empty() {
        writeln!(
            printer.stderr(),
            "{}{} Package(s) not found for: {}",
            "warning".yellow().bold(),
            ":".bold(),
            missing.iter().join(", ").bold()
        )?;
    }

    let distributions = distributions.iter().flatten().collect_vec();

    // Like `pip`, if no packages were found, return a failure.
    if distributions.is_empty() {
        return Ok(ExitStatus::Failure);
    }

    // Print the information for each package.
    for (i, distribution) in distributions.iter().enumerate() {
        if i > 0 {
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

        if let Some(url) = distribution.as_editable() {
            let path = url.to_file_path().unwrap().simplified_display().to_string();
            writeln!(printer.stdout(), "Editable project location: {path}")?;
        }

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

            let required_by = site_packages
                .iter()
                .filter(|dist| {
                    dist.name() != distribution.name()
                        && dist.metadata().is_ok_and(|metadata| {
                            metadata
                                .requires_dist
                                .into_iter()
                                .filter(|req| req.evaluate_markers(markers, &[]))
                                .any(|req| &req.name == distribution.name())
                        })
                })
                .map(distribution_types::Name::name)
                .collect::<BTreeSet<_>>();
            if required_by.is_empty() {
                writeln!(printer.stdout(), "Required-by:")?;
            } else {
                writeln!(
                    printer.stdout(),
                    "Required-by: {}",
                    required_by.into_iter().join(", ")
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
