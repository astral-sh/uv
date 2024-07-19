use fs_err as fs;
use std::{io, path::Path, path::PathBuf};
use tracing::debug;

use crate::PythonRequest;

/// The file name for Python version pins.
pub static PYTHON_VERSION_FILENAME: &str = ".python-version";

/// The file name for multiple Python version declarations.
pub static PYTHON_VERSIONS_FILENAME: &str = ".python-versions";

/// Read [`PythonRequest`]s from a version file, if present.
///
/// Prefers `.python-versions` then `.python-version`.
/// If only one Python version is desired, use [`request_from_version_file_in`] which prefers the `.python-version` file.
pub async fn requests_from_version_file_in(
    root: &PathBuf,
) -> Result<Option<Vec<PythonRequest>>, io::Error> {
    if let Some(versions) = read_versions_file_in(root).await? {
        Ok(Some(
            versions
                .into_iter()
                .map(|version| PythonRequest::parse(&version))
                .collect(),
        ))
    } else if let Some(version) = read_version_file_in(root).await? {
        Ok(Some(vec![PythonRequest::parse(&version)]))
    } else {
        Ok(None)
    }
}

/// Read a [`PythonRequest`] from a version file, if present.
///
/// Prefers `.python-version` then the first entry of `.python-versions`.
/// If multiple Python versions are desired, use [`requests_from_version_file_in`] instead.
pub async fn request_from_version_file_in(
    root: &PathBuf,
) -> Result<Option<PythonRequest>, io::Error> {
    if let Some(version) = read_version_file_in(root).await? {
        Ok(Some(PythonRequest::parse(&version)))
    } else if let Some(versions) = read_versions_file_in(root).await? {
        Ok(versions
            .into_iter()
            .next()
            .inspect(|_| debug!("Using the first version from `.python-versions`"))
            .map(|version| PythonRequest::parse(&version)))
    } else {
        Ok(None)
    }
}

/// Read a `.python-versions` file in `root` folder if present.
async fn read_versions_file_in(root: impl AsRef<Path>) -> Result<Option<Vec<String>>, io::Error> {
    let version_file = root.as_ref().join(PYTHON_VERSIONS_FILENAME);
    if !version_file.try_exists()? {
        return Ok(None);
    }
    debug!("Reading requests from `{}`", version_file.display());
    let lines: Vec<String> = fs::tokio::read_to_string(version_file)
        .await?
        .lines()
        .map(ToString::to_string)
        .collect();
    Ok(Some(lines))
}

/// Read a `.python-version` file in `root` folder if present.
async fn read_version_file_in(root: impl AsRef<Path>) -> Result<Option<String>, io::Error> {
    let version_file = root.as_ref().join(PYTHON_VERSION_FILENAME);
    if !version_file.try_exists()? {
        return Ok(None);
    }
    debug!("Reading requests from `{}`", version_file.display());
    Ok(fs::tokio::read_to_string(version_file)
        .await?
        .lines()
        .next()
        .map(ToString::to_string))
}

/// Read [`PythonRequest`]s from a version file, if present.
///
/// Prefers `.python-versions` then `.python-version`.
/// If only one Python version is desired, use [`request_from_version_file`] which prefers the `.python-version` file.
pub async fn requests_from_version_file() -> Result<Option<Vec<PythonRequest>>, io::Error> {
    if let Some(versions) = read_versions_file().await? {
        Ok(Some(
            versions
                .into_iter()
                .map(|version| PythonRequest::parse(&version))
                .collect(),
        ))
    } else if let Some(version) = read_version_file().await? {
        Ok(Some(vec![PythonRequest::parse(&version)]))
    } else {
        Ok(None)
    }
}

/// Read a [`PythonRequest`] from a version file, if present.
///
/// Prefers `.python-version` then the first entry of `.python-versions`.
/// If multiple Python versions are desired, use [`requests_from_version_file`] instead.
pub async fn request_from_version_file() -> Result<Option<PythonRequest>, io::Error> {
    if let Some(version) = read_version_file().await? {
        Ok(Some(PythonRequest::parse(&version)))
    } else if let Some(versions) = read_versions_file().await? {
        Ok(versions
            .into_iter()
            .next()
            .inspect(|_| debug!("Using the first version from `.python-versions`"))
            .map(|version| PythonRequest::parse(&version)))
    } else {
        Ok(None)
    }
}

pub fn versions_file_exists() -> Result<bool, io::Error> {
    PathBuf::from(PYTHON_VERSIONS_FILENAME).try_exists()
}

async fn read_versions_file() -> Result<Option<Vec<String>>, io::Error> {
    if !versions_file_exists()? {
        return Ok(None);
    }
    debug!("Reading requests from `{PYTHON_VERSIONS_FILENAME}`");
    let lines: Vec<String> = fs::tokio::read_to_string(PYTHON_VERSIONS_FILENAME)
        .await?
        .lines()
        .map(ToString::to_string)
        .collect();
    Ok(Some(lines))
}

pub fn version_file_exists() -> Result<bool, io::Error> {
    PathBuf::from(PYTHON_VERSION_FILENAME).try_exists()
}

async fn read_version_file() -> Result<Option<String>, io::Error> {
    if !version_file_exists()? {
        return Ok(None);
    }
    debug!("Reading requests from `{PYTHON_VERSION_FILENAME}`");
    Ok(fs::tokio::read_to_string(PYTHON_VERSION_FILENAME)
        .await?
        .lines()
        .next()
        .map(ToString::to_string))
}
