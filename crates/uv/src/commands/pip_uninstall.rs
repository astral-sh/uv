use std::fmt::Write;

use anyhow::Result;
use owo_colors::OwoColorize;
use tracing::debug;

use distribution_types::{InstalledMetadata, Name};
use platform_host::Platform;
use uv_cache::Cache;
use uv_client::Connectivity;
use uv_fs::Simplified;
use uv_interpreter::PythonEnvironment;

use crate::commands::{elapsed, ExitStatus};
use crate::printer::Printer;
use crate::requirements::{RequirementsSource, RequirementsSpecification};

/// Uninstall packages from the current environment.
pub(crate) async fn pip_uninstall(
    sources: &[RequirementsSource],
    python: Option<String>,
    system: bool,
    break_system_packages: bool,
    cache: Cache,
    connectivity: Connectivity,
    printer: Printer,
) -> Result<ExitStatus> {
    let start = std::time::Instant::now();

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
    } = RequirementsSpecification::from_simple_sources(sources, connectivity).await?;

    // Detect the current Python interpreter.
    let platform = Platform::current()?;
    let venv = if let Some(python) = python.as_ref() {
        PythonEnvironment::from_requested_python(python, &platform, &cache)?
    } else if system {
        PythonEnvironment::from_default_python(&platform, &cache)?
    } else {
        PythonEnvironment::from_virtualenv(platform, &cache)?
    };
    debug!(
        "Using Python {} environment at {}",
        venv.interpreter().python_version(),
        venv.python_executable().simplified_display().cyan(),
    );

    // If the environment is externally managed, abort.
    if let Some(externally_managed) = venv.interpreter().is_externally_managed() {
        if break_system_packages {
            debug!("Ignoring externally managed environment due to `--break-system-packages`");
        } else {
            return if let Some(error) = externally_managed.into_error() {
                Err(anyhow::anyhow!(
                    "The interpreter at {} is externally managed, and indicates the following:\n\n{}\n\nConsider creating a virtual environment with `uv venv`.",
                    venv.root().simplified_display().cyan(),
                    textwrap::indent(&error, "  ").green(),
                ))
            } else {
                Err(anyhow::anyhow!(
                    "The interpreter at {} is externally managed. Instead, create a virtual environment with `uv venv`.",
                    venv.root().simplified_display().cyan()
                ))
            };
        }
    }

    let _lock = venv.lock()?;

    // Index the current `site-packages` directory.
    let site_packages = uv_installer::SitePackages::from_executable(&venv)?;

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

    // Map to the local distributions.
    let distributions = {
        let mut distributions = Vec::with_capacity(packages.len() + editables.len());

        // Identify all packages that are installed.
        for package in &packages {
            let installed = site_packages.get_packages(package);
            if installed.is_empty() {
                writeln!(
                    printer.stderr(),
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
                    printer.stderr(),
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

    if distributions.is_empty() {
        writeln!(
            printer.stderr(),
            "{}{} No packages to uninstall.",
            "warning".yellow().bold(),
            ":".bold(),
        )?;
        return Ok(ExitStatus::Success);
    }

    // Uninstall each package.
    for distribution in &distributions {
        let summary = uv_installer::uninstall(distribution).await?;
        debug!(
            "Uninstalled {} ({} file{}, {} director{})",
            distribution.name(),
            summary.file_count,
            if summary.file_count == 1 { "" } else { "s" },
            summary.dir_count,
            if summary.dir_count == 1 { "y" } else { "ies" },
        );
    }

    writeln!(
        printer.stderr(),
        "{}",
        format!(
            "Uninstalled {} in {}",
            format!(
                "{} package{}",
                distributions.len(),
                if distributions.len() == 1 { "" } else { "s" }
            )
            .bold(),
            elapsed(start.elapsed())
        )
        .dimmed()
    )?;

    for distribution in distributions {
        writeln!(
            printer.stderr(),
            " {} {}{}",
            "-".red(),
            distribution.name().as_ref().bold(),
            distribution.installed_version().to_string().dimmed()
        )?;
    }

    Ok(ExitStatus::Success)
}
