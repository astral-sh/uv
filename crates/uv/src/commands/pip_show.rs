use std::cmp::max;
use std::fmt::Write;

use anstream::println;
use anyhow::Result;
use itertools::Itertools;
use owo_colors::OwoColorize;
use tracing::debug;
use unicode_width::UnicodeWidthStr;

use distribution_types::Name;
use platform_host::Platform;
use uv_cache::Cache;
use uv_fs::Simplified;
use uv_installer::SitePackages;
use uv_interpreter::PythonEnvironment;
use uv_normalize::PackageName;

use crate::commands::ExitStatus;
use crate::printer::Printer;
use crate::requirements::{RequirementsSource, RequirementsSpecification};

/// Show information about one or more installed packages.
#[allow(clippy::too_many_arguments, clippy::fn_params_excessive_bools)]
pub(crate) fn pip_show(
    sources: &[RequirementsSource],
    strict: bool,
    exclude_editable: bool,
    exclude: &[PackageName],
    python: Option<&str>,
    system: bool,
    cache: &Cache,
    mut printer: Printer,
) -> Result<ExitStatus> {
    // Read all requirements from the provided sources.
    let RequirementsSpecification {
        project: _project,
        requirements,
        constraints: _constraints,
        overrides: _overrides,
        editables,
        index_url: _index_url,
        extra_index_urls: _extra_index_urls,
        no_index: _no_index,
        find_links: _find_links,
        extras: _extras,
    } = RequirementsSpecification::from_simple_sources(sources)?;
    // Sort and deduplicate the packages, which are keyed by name.
    let packages = {
        let mut packages = requirements
            .into_iter()
            .map(|requirement| requirement.name)
            .collect::<Vec<_>>();
        packages.sort_unstable();
        packages.dedup();
        packages
    };

    // Sort and deduplicate the editable packages, which are keyed by URL rather than package name.
    let editables = {
        let mut editables = editables
            .iter()
            .map(requirements_txt::EditableRequirement::raw)
            .collect::<Vec<_>>();
        editables.sort_unstable();
        editables.dedup();
        editables
    };

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
    // Map to the local distributions.
    let distributions = {
        let mut distributions = Vec::with_capacity(packages.len() + editables.len());

        // Identify all packages that are installed.
        for package in &packages {
            let installed = site_packages.get_packages(package);
            if installed.is_empty() {
                writeln!(
                    printer,
                    "{}{} Skipping {} as it is not installed.",
                    "warning".yellow().bold(),
                    ":".bold(),
                    package.as_ref().bold()
                )?;
            } else {
                distributions.extend(installed);
            }
        }

        // Identify all editables that are installed.
        for editable in &editables {
            let installed = site_packages.get_editables(editable);
            if installed.is_empty() {
                writeln!(
                    printer,
                    "{}{} Skipping {} as it is not installed.",
                    "warning".yellow().bold(),
                    ":".bold(),
                    editable.as_ref().bold()
                )?;
            } else {
                distributions.extend(installed);
            }
        }

        // Deduplicate, since a package could be listed both by name and editable URL.
        distributions.sort_unstable_by_key(|dist| dist.path());
        distributions.dedup_by_key(|dist| dist.path());
        distributions
    };

    for distribution in &distributions {
        println!("{:?}", distribution);
        println!("Name: {}", distribution.name().to_string());
        println!("Version: {}", distribution.version().to_string());
        println!(
            "Location: {}",
            distribution.path().parent().unwrap().display()
        );
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
