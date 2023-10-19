use std::fmt::Write;
use std::path::Path;

use anyhow::Result;
use colored::Colorize;
use itertools::Itertools;
use tracing::debug;

use pep508_rs::Requirement;
use platform_host::Platform;
use puffin_interpreter::PythonExecutable;
use puffin_package::package_name::PackageName;

use crate::commands::{elapsed, ExitStatus};
use crate::printer::Printer;
use crate::requirements::RequirementsSource;

/// Uninstall packages from the current environment.
pub(crate) async fn pip_uninstall(
    sources: &[RequirementsSource],
    cache: Option<&Path>,
    mut printer: Printer,
) -> Result<ExitStatus> {
    let start = std::time::Instant::now();

    // Read all requirements from the provided sources.
    let requirements = sources
        .iter()
        .map(RequirementsSource::requirements)
        .flatten_ok()
        .collect::<Result<Vec<Requirement>>>()?;

    // Detect the current Python interpreter.
    let platform = Platform::current()?;
    let python = PythonExecutable::from_env(platform, cache)?;
    debug!(
        "Using Python interpreter: {}",
        python.executable().display()
    );

    // Index the current `site-packages` directory.
    let site_packages = puffin_installer::SitePackages::from_executable(&python).await?;

    // Sort and deduplicate the requirements.
    let packages = {
        let mut packages = requirements
            .into_iter()
            .map(|requirement| PackageName::normalize(requirement.name))
            .collect::<Vec<_>>();
        packages.sort_unstable();
        packages.dedup();
        packages
    };

    // Map to the local distributions.
    let distributions = packages
        .iter()
        .filter_map(|package| {
            if let Some(distribution) = site_packages.get(package) {
                Some(distribution)
            } else {
                let _ = writeln!(
                    printer,
                    "{}{} Skipping {} as it is not installed.",
                    "warning".yellow().bold(),
                    ":".bold(),
                    package.as_ref().bold()
                );
                None
            }
        })
        .collect::<Vec<_>>();

    if distributions.is_empty() {
        writeln!(
            printer,
            "{}{} No packages to uninstall.",
            "warning".yellow().bold(),
            ":".bold(),
        )?;
        return Ok(ExitStatus::Success);
    }

    // Uninstall each package.
    for distribution in &distributions {
        let summary = puffin_installer::uninstall(distribution).await?;
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
        printer,
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
            printer,
            " {} {}{}",
            "-".red(),
            distribution.name().as_ref().white().bold(),
            format!("@{}", distribution.version()).dimmed()
        )?;
    }

    Ok(ExitStatus::Success)
}
