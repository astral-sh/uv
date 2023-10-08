use std::path::Path;
use std::str::FromStr;

use anyhow::Result;
use bitflags::bitflags;
use tracing::{debug, info};

use platform_host::Platform;
use platform_tags::Tags;
use puffin_client::PypiClientBuilder;
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

    // Remove any already-installed packages.
    let requirements = if flags.intersects(SyncFlags::IGNORE_INSTALLED) {
        requirements
    } else {
        let site_packages = SitePackages::from_executable(&python).await?;
        requirements.filter(|requirement| {
            let package = PackageName::normalize(&requirement.name);
            if let Some(version) = site_packages.get(&package) {
                #[allow(clippy::print_stdout)]
                {
                    info!("Requirement already satisfied: {package} ({version})");
                }
                false
            } else {
                true
            }
        })
    };

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

    // Determine the current environment markers.
    let markers = python.markers();

    // Determine the compatible platform tags.
    let tags = Tags::from_env(python.platform(), python.simple_version())?;

    // Instantiate a client.
    let client = {
        let mut pypi_client = PypiClientBuilder::default();
        if let Some(cache) = cache {
            pypi_client = pypi_client.cache(cache);
        }
        pypi_client.build()
    };

    // Resolve the dependencies.
    let resolution = puffin_resolver::resolve(
        &requirements,
        markers,
        &tags,
        &client,
        puffin_resolver::ResolveFlags::NO_DEPS,
    )
    .await?;

    // Install into the current environment.
    let wheels = resolution.into_files().collect::<Vec<_>>();
    puffin_installer::install(&wheels, &python, &client, cache).await?;

    let s = if requirements.len() == 1 { "" } else { "s" };
    info!(
        "Installed {} package{} in {}",
        requirements.len(),
        s,
        elapsed(start.elapsed())
    );

    Ok(ExitStatus::Success)
}
