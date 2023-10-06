use std::path::Path;
use std::str::FromStr;

use anyhow::Result;
use tracing::debug;

use puffin_client::PypiClientBuilder;
use puffin_interpreter::PythonExecutable;
use puffin_platform::tags::Tags;
use puffin_platform::Platform;

use crate::commands::ExitStatus;

/// Install a set of locked requirements into the current Python environment.
pub(crate) async fn sync(src: &Path, cache: Option<&Path>) -> Result<ExitStatus> {
    // Read the `requirements.txt` from disk.
    let requirements_txt = std::fs::read_to_string(src)?;

    // Parse the `requirements.txt` into a list of requirements.
    let requirements = puffin_package::requirements::Requirements::from_str(&requirements_txt)?;

    // Detect the current Python interpreter.
    let platform = Platform::current()?;
    let python = PythonExecutable::from_env(&platform)?;
    debug!(
        "Using Python interpreter: {}",
        python.executable().display()
    );

    // Determine the current environment markers.
    let markers = python.markers();

    // Determine the compatible platform tags.
    let tags = Tags::from_env(&platform, python.version())?;

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
        puffin_resolver::Flags::NO_DEPS,
    )
    .await?;

    // Install into the current environment.
    let wheels = resolution.into_files().collect::<Vec<_>>();
    puffin_installer::install(&wheels, &python, &client).await?;

    #[allow(clippy::print_stdout)]
    {
        println!("Installed {} wheels", wheels.len());
    }

    Ok(ExitStatus::Success)
}
