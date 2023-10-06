use std::str::FromStr;

use anyhow::Result;
use install_wheel_rs::{install_wheel, InstallLocation};
use url::Url;

use puffin_client::{File, PypiClient};
use puffin_interpreter::PythonExecutable;

/// Install a set of wheels into a Python virtual environment.
pub async fn install(
    wheels: &[File],
    python: &PythonExecutable,
    client: &PypiClient,
) -> Result<()> {
    // Create a temporary directory, in which we'll store the wheels.
    let tmp_dir = tempfile::tempdir()?;

    // Download each wheel.
    // TODO(charlie): Store these in a content-addressed cache.
    // TODO(charlie): Use channels to efficiently stream-and-install.
    for wheel in wheels {
        let url = Url::parse(&wheel.url)?;
        let reader = client.stream_external(&url).await?;

        // TODO(charlie): Stream the unzip.
        let mut writer =
            async_std::fs::File::create(tmp_dir.path().join(&wheel.hashes.sha256)).await?;
        async_std::io::copy(reader, &mut writer).await?;
    }

    // Install each wheel.
    // TODO(charlie): Use channels to efficiently stream-and-install.
    let location = InstallLocation::Venv {
        venv_base: python.venv().to_path_buf(),
        python_version: python.simple_version(),
    };
    let locked_dir = location.acquire_lock()?;
    for wheel in wheels {
        let path = tmp_dir.path().join(&wheel.hashes.sha256);
        let filename = install_wheel_rs::WheelFilename::from_str(&wheel.filename)?;

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

    Ok(())
}
