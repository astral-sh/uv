
use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::Result;
use async_std::fs::File;
use futures::io::Cursor;
use tracing::debug;
use url::Url;
use install_wheel_rs::{install_wheel, InstallLocation};
use puffin_client::PypiClientBuilder;

use puffin_interpreter::PythonExecutable;
use puffin_package::wheel::WheelFilename;
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

    // Download each wheel.
    // TODO(charlie): Store these in a content-addressed cache.
    for (name, package) in resolution.iter() {
        let url = Url::parse(package.url())?;
        let reader = client.stream_external(&url).await?;

        // TODO(charlie): Stream the unzip.
        let mut writer =  File::create(format!("{name}.whl")).await?;
        async_std::io::copy(reader, &mut writer).await?;
    }

    // Install each wheel.
    for (name, package) in resolution.iter() {
        let filename = WheelFilename::from_str(package.url())?;
        let path = PathBuf::from(format!("{name}.whl"));
        let reader = File::open(&path).await?;
        let mut writer = Cursor::new(Vec::new());
        install_wheel(reader, &mut writer, InstallLocation::User).await?;
    }

    Ok(ExitStatus::Success)
}
