use std::fmt::Write;
use std::path::Path;
use std::str::FromStr;

use anyhow::Result;
use bitflags::bitflags;
use colored::Colorize;
use tracing::debug;

use platform_host::Platform;
use platform_tags::Tags;
use puffin_client::PypiClientBuilder;
use puffin_installer::{Distribution, LocalDistribution, LocalIndex, RemoteDistribution};
use puffin_interpreter::{PythonExecutable, SitePackages};
use puffin_package::package_name::PackageName;
use puffin_package::requirements::Requirements;

use crate::commands::reporters::{DownloadReporter, InstallReporter, ResolverReporter};
use crate::commands::{elapsed, ExitStatus};
use crate::printer::Printer;

bitflags! {
    #[derive(Debug, Copy, Clone, Default)]
    pub struct SyncFlags: u8 {
        /// Ignore any installed packages, forcing a re-installation.
        const IGNORE_INSTALLED = 1 << 0;
    }
}

/// Install a set of locked requirements into the current Python environment.
pub(crate) async fn sync(
    src: &Path,
    cache: Option<&Path>,
    flags: SyncFlags,
    mut printer: Printer,
) -> Result<ExitStatus> {
    let start = std::time::Instant::now();

    // Read the `requirements.txt` from disk.
    let requirements_txt = std::fs::read_to_string(src)?;

    // Parse the `requirements.txt` into a list of requirements.
    let requirements = Requirements::from_str(&requirements_txt)?;
    let initial_requirements = requirements.len();

    // Detect the current Python interpreter.
    let platform = Platform::current()?;
    let python = PythonExecutable::from_env(platform, cache)?;
    debug!(
        "Using Python interpreter: {}",
        python.executable().display()
    );

    // Determine the current environment markers.
    let markers = python.markers();
    let tags = Tags::from_env(python.platform(), python.simple_version())?;

    // Index all the already-installed packages in site-packages.
    let site_packages = if flags.intersects(SyncFlags::IGNORE_INSTALLED) {
        SitePackages::default()
    } else {
        SitePackages::from_executable(&python).await?
    };

    // Index all the already-downloaded wheels in the cache.
    let local_index = if let Some(cache) = cache {
        LocalIndex::from_directory(cache).await?
    } else {
        LocalIndex::default()
    };

    let requirements = requirements
        .iter()
        .filter_map(|requirement| {
            let package = PackageName::normalize(&requirement.name);

            // Filter out already-installed packages.
            if let Some(dist_info) = site_packages.get(&package) {
                debug!(
                    "Requirement already satisfied: {} ({})",
                    package,
                    dist_info.version()
                );
                return None;
            }

            // Identify any locally-available distributions that satisfy the requirement.
            if let Some(distribution) = local_index
                .get(&package)
                .filter(|dist| requirement.is_satisfied_by(dist.version()))
            {
                debug!(
                    "Requirement already cached: {} ({})",
                    distribution.name(),
                    distribution.version()
                );
                return Some(Requirement::Local(distribution.clone()));
            }

            debug!("Identified uncached requirement: {}", requirement);
            Some(Requirement::Remote(requirement.clone()))
        })
        .collect::<Vec<_>>();

    if requirements.is_empty() {
        let s = if initial_requirements == 1 { "" } else { "s" };
        writeln!(
            printer,
            "{}",
            format!(
                "Audited {} in {}",
                format!("{initial_requirements} package{s}").bold(),
                elapsed(start.elapsed())
            )
            .dimmed()
        )?;

        return Ok(ExitStatus::Success);
    }

    let client = {
        let mut pypi_client = PypiClientBuilder::default();
        if let Some(cache) = cache {
            pypi_client = pypi_client.cache(cache);
        }
        pypi_client.build()
    };

    let num_wheels = requirements.len();
    let num_remote = requirements
        .iter()
        .filter(|wheel| matches!(wheel, Requirement::Remote(_)))
        .count();

    // Resolve the dependencies.
    let resolver = puffin_resolver::Resolver::new(markers, &tags, &client)
        .with_reporter(ResolverReporter::from(printer).with_length(num_remote as u64));
    let resolution = resolver
        .resolve(
            requirements
                .iter()
                .filter_map(|requirement| match requirement {
                    Requirement::Remote(requirement) => Some(requirement),
                    Requirement::Local(_) => None,
                }),
            puffin_resolver::ResolveFlags::NO_DEPS,
        )
        .await?;

    let s = if resolution.len() == 1 { "" } else { "s" };
    writeln!(
        printer,
        "{}",
        format!(
            "Resolved {} in {}",
            format!("{} package{}", resolution.len(), s).bold(),
            elapsed(start.elapsed())
        )
        .dimmed()
    )?;

    let wheels = requirements
        .into_iter()
        .filter_map(|requirement| match requirement {
            Requirement::Remote(_) => None,
            Requirement::Local(distribution) => Some(Ok(Distribution::Local(distribution))),
        })
        .chain(
            resolution
                .into_files()
                .map(|file| Ok(Distribution::Remote(RemoteDistribution::from_file(file)?))),
        )
        .collect::<Result<Vec<_>>>()?;

    // Download any missing distributions.
    let downloader = puffin_installer::Downloader::new(&python, &client, cache)
        .with_reporter(DownloadReporter::from(printer).with_length(num_remote as u64));
    let download_set = downloader.download(&wheels).await?;

    let s = if num_remote == 1 { "" } else { "s" };
    writeln!(
        printer,
        "{}",
        format!(
            "Downloaded {} in {}",
            format!("{num_remote} package{s}").bold(),
            elapsed(start.elapsed())
        )
        .dimmed()
    )?;

    // Install the resolved distributions.
    puffin_installer::Installer::from(download_set)
        .with_reporter(InstallReporter::from(printer).with_length(num_wheels as u64))
        .install()?;

    let s = if num_wheels == 1 { "" } else { "s" };
    writeln!(
        printer,
        "{}",
        format!(
            "Installed {} in {}",
            format!("{num_wheels} package{s}").bold(),
            elapsed(start.elapsed())
        )
        .dimmed()
    )?;

    for wheel in wheels {
        writeln!(
            printer,
            " {} {}{}",
            "+".green(),
            wheel.name().white().bold(),
            format!("@{}", wheel.version()).dimmed()
        )?;
    }

    Ok(ExitStatus::Success)
}

#[derive(Debug)]
enum Requirement {
    /// A requirement that must be downloaded from PyPI.
    Remote(pep508_rs::Requirement),
    /// A requirement that is already available locally.
    Local(LocalDistribution),
}
