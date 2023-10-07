use std::path::Path;
use std::str::FromStr;

use anyhow::Result;
use tokio::task::JoinSet;
use tokio_util::compat::FuturesAsyncReadCompatExt;
use url::Url;

use install_wheel_rs::{install_wheel, InstallLocation};
use puffin_client::{File, PypiClient};
use puffin_interpreter::PythonExecutable;
use wheel_filename::WheelFilename;

/// Install a set of wheels into a Python virtual environment.
pub async fn install(
    wheels: &[File],
    python: &PythonExecutable,
    client: &PypiClient,
) -> Result<()> {
    // Create a temporary directory, in which we'll store the wheels.
    let tmp_dir = tempfile::tempdir()?;

    // Download the wheels in parallel.
    let mut downloads = JoinSet::new();
    for wheel in wheels {
        downloads.spawn(do_download(
            wheel.clone(),
            client.clone(),
            tmp_dir.path().join(&wheel.hashes.sha256),
        ));
    }
    while let Some(result) = downloads.join_next().await.transpose()? {
        result?;
    }

    // Install each wheel.
    let location = InstallLocation::Venv {
        venv_base: python.venv().to_path_buf(),
        python_version: python.simple_version(),
    };
    let locked_dir = location.acquire_lock()?;
    for wheel in wheels {
        let path = tmp_dir.path().join(&wheel.hashes.sha256);
        let filename = WheelFilename::from_str(&wheel.filename)?;

        // TODO(charlie): Should this be async?
        install_wheel(
            &locked_dir,
            std::fs::File::open(path)?,
            &filename,
            false,
            false,
            &[],
            "",
            python.executable(),
        )?;
    }

    Ok(())
}

/// Download a wheel to a given path.
async fn do_download(wheel: File, client: PypiClient, path: impl AsRef<Path>) -> Result<File> {
    // TODO(charlie): Store these in a content-addressed cache.
    let url = Url::parse(&wheel.url)?;
    let reader = client.stream_external(&url).await?;

    // TODO(charlie): Stream the unzip.
    let mut writer = tokio::fs::File::create(path).await?;
    tokio::io::copy(&mut reader.compat(), &mut writer).await?;

    Ok(wheel)
}
