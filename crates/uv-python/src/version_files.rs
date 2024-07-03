use fs_err as fs;
use std::{io, path::PathBuf};
use tracing::debug;

use crate::PythonRequest;

/// Read [`PythonRequest`]s from a version file, if present.
///
/// Prefers `.python-versions` then `.python-version`.
/// If only one Python version is desired, use [`request_from_version_files`] which prefers the `.python-version` file.
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
/// If multiple Python versions are desired, use [`requests_from_version_files`] instead.
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

async fn read_versions_file() -> Result<Option<Vec<String>>, io::Error> {
    if !PathBuf::from(".python-versions").try_exists()? {
        return Ok(None);
    }
    debug!("Reading requests from `.python-versions`");
    let lines: Vec<String> = fs::tokio::read_to_string(".python-versions")
        .await?
        .lines()
        .map(ToString::to_string)
        .collect();
    Ok(Some(lines))
}

async fn read_version_file() -> Result<Option<String>, io::Error> {
    if !PathBuf::from(".python-version").try_exists()? {
        return Ok(None);
    }
    debug!("Reading requests from `.python-version`");
    Ok(fs::tokio::read_to_string(".python-version")
        .await?
        .lines()
        .next()
        .map(ToString::to_string))
}
