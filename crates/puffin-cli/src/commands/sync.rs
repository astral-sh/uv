use std::path::Path;
use std::str::FromStr;

use anyhow::Result;
use bitflags::bitflags;
use tracing::{debug, info};

use platform_host::Platform;
use platform_tags::Tags;
use puffin_client::PypiClientBuilder;
use puffin_installer::{Distribution, LocalDistribution, LocalIndex, RemoteDistribution};
use puffin_interpreter::{PythonExecutable, SitePackages};
use puffin_package::package_name::PackageName;
use puffin_package::requirements::Requirements;

use crate::commands::{elapsed, ExitStatus};

bitflags! {
    #[derive(Debug, Copy, Clone, Default)]
    pub struct SyncFlags: u8 {
        /// Ignore any installed packages, forcing a re-installation.
        const IGNORE_INSTALLED = 1 << 0;
    }
}

/// Install a set of locked requirements into the current Python environment.
pub(crate) async fn sync(src: &Path, cache: Option<&Path>, flags: SyncFlags) -> Result<ExitStatus> {
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
            if let Some(version) = site_packages.get(&package) {
                info!("Requirement already satisfied: {package} ({version})");
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
        info!(
            "Audited {} package{} in {}",
            initial_requirements,
            s,
            elapsed(start.elapsed())
        );
        return Ok(ExitStatus::Success);
    }

    // Resolve the dependencies.
    let client = {
        let mut pypi_client = PypiClientBuilder::default();
        if let Some(cache) = cache {
            pypi_client = pypi_client.cache(cache);
        }
        pypi_client.build()
    };
    let resolution = puffin_resolver::resolve(
        requirements
            .iter()
            .filter_map(|requirement| match requirement {
                Requirement::Remote(requirement) => Some(requirement),
                Requirement::Local(_) => None,
            }),
        markers,
        &tags,
        &client,
        puffin_resolver::ResolveFlags::NO_DEPS,
    )
    .await?;

    // Install the resolved distributions.
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
    puffin_installer::install(&wheels, &python, &client, cache).await?;

    let s = if wheels.len() == 1 { "" } else { "s" };
    info!(
        "Installed {} package{} in {}",
        wheels.len(),
        s,
        elapsed(start.elapsed())
    );

    Ok(ExitStatus::Success)
}

#[derive(Debug)]
enum Requirement {
    /// A requirement that must be downloaded from PyPI.
    Remote(pep508_rs::Requirement),
    /// A requirement that is already available locally.
    Local(LocalDistribution),
}
