use std::path::Path;
use std::str::FromStr;

use anyhow::Result;
use async_std::fs::File;
use tracing::debug;
use url::Url;

use install_wheel_rs::{install_wheel, InstallLocation};
use puffin_client::PypiClientBuilder;
use puffin_interpreter::PythonExecutable;
use puffin_platform::tags::Tags;
use puffin_platform::Platform;
use puffin_resolve::resolve;

use crate::commands::ExitStatus;

pub(crate) async fn install(src: &Path, cache: Option<&Path>) -> Result<ExitStatus> {
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
    let resolution = resolve(&requirements, markers, &tags, &client).await?;

    // Create a temporary directory, in which we'll store the wheels.
    let tmp_dir = tempfile::tempdir()?;

    // Download each wheel.
    // TODO(charlie): Store these in a content-addressed cache.
    // TODO(charlie): Use channels to efficiently stream-and-install.
    for (name, package) in resolution.iter() {
        let url = Url::parse(package.url())?;
        let reader = client.stream_external(&url).await?;

        // TODO(charlie): Stream the unzip.
        let mut writer = File::create(tmp_dir.path().join(format!("{name}.whl"))).await?;
        async_std::io::copy(reader, &mut writer).await?;
    }

    // Install each wheel.
    // TODO(charlie): Use channels to efficiently stream-and-install.
    let location = InstallLocation::Venv {
        venv_base: python.venv().to_path_buf(),
        python_version: python.simple_version(),
    };
    let locked_dir = location.acquire_lock()?;
    for (name, package) in resolution.iter() {
        let path = tmp_dir.path().join(format!("{name}.whl"));
        let filename = install_wheel_rs::WheelFilename::from_str(package.filename())?;

        // TODO(charlie): Should this be async?
        install_wheel(
            &locked_dir,
            std::fs::File::open(path)?,
            filename,
            false,
            &[],
            "",
            python.executable(),
        )?;
    }

    Ok(ExitStatus::Success)
}
