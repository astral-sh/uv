use std::fmt::Write;

use anyhow::Result;
use itertools::{Either, Itertools};
use owo_colors::OwoColorize;
use rustc_hash::FxHashMap;
use tracing::debug;

use distribution_types::{Diagnostic, Name};
use uv_cache::Cache;
use uv_configuration::PreviewMode;
use uv_fs::Simplified;
use uv_installer::SitePackages;
use uv_normalize::PackageName;
use uv_python::{EnvironmentPreference, PythonEnvironment, PythonRequest};

use crate::commands::ExitStatus;
use crate::printer::Printer;

/// Show information about one or more installed packages.
pub(crate) fn pip_show(
    mut packages: Vec<PackageName>,
    strict: bool,
    python: Option<&str>,
    system: bool,
    _preview: PreviewMode,
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

    // Determine the markers to use for resolution.
    let markers = environment.interpreter().markers();

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

    // Since Requires and Required-by fields need data parsed from metadata, especially the
    // Required-by field which needs to iterate over other installed packages' metadata.
    // To prevent the need to parse metadata repeatedly when multiple packages need to be shown,
    // we parse the metadata once and collect the needed data beforehand.
    let mut requires_map = FxHashMap::default();
    // For Requires field
    for dist in &distributions {
        if let Ok(metadata) = dist.metadata() {
            requires_map.insert(
                dist.name(),
                metadata
                    .requires_dist
                    .into_iter()
                    .filter(|req| req.evaluate_markers(markers, &[]))
                    .map(|req| req.name)
                    .sorted_unstable()
                    .dedup()
                    .collect_vec(),
            );
        }
    }
    // For Required-by field
    if !requires_map.is_empty() {
        for installed in site_packages.iter() {
            if requires_map.contains_key(installed.name()) {
                continue;
            }
            if let Ok(metadata) = installed.metadata() {
                let requires = metadata
                    .requires_dist
                    .into_iter()
                    .filter(|req| req.evaluate_markers(markers, &[]))
                    .map(|req| req.name)
                    .collect_vec();
                if !requires.is_empty() {
                    requires_map.insert(installed.name(), requires);
                }
            }
        }
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

        if let Some(path) = distribution
            .as_editable()
            .and_then(|url| url.to_file_path().ok())
        {
            writeln!(
                printer.stdout(),
                "Editable project location: {}",
                path.simplified_display()
            )?;
        }

        // If available, print the requirements.
        if let Some(requires) = requires_map.get(distribution.name()) {
            if requires.is_empty() {
                writeln!(printer.stdout(), "Requires:")?;
            } else {
                writeln!(printer.stdout(), "Requires: {}", requires.iter().join(", "))?;
            }

            let required_by = requires_map
                .iter()
                .filter(|(name, pkgs)| {
                    **name != distribution.name()
                        && pkgs.iter().any(|pkg| pkg == distribution.name())
                })
                .map(|(name, _)| name)
                .sorted_unstable()
                .dedup()
                .collect_vec();
            if required_by.is_empty() {
                writeln!(printer.stdout(), "Required-by:")?;
            } else {
                writeln!(
                    printer.stdout(),
                    "Required-by: {}",
                    required_by.into_iter().join(", "),
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
