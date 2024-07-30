use std::path::Path;

use fs_err as fs;
use tracing::debug;

use crate::PythonRequest;

/// The file name for Python version pins.
pub static PYTHON_VERSION_FILENAME: &str = ".python-version";

/// The file name for multiple Python version declarations.
pub static PYTHON_VERSIONS_FILENAME: &str = ".python-versions";

/// Read [`PythonRequest`]s from a version file, if present.
///
/// Prefers `.python-versions` then `.python-version`.
/// If only one Python version is desired, use [`request_from_version_files`] which prefers the `.python-version` file.
pub async fn requests_from_version_file(
    directory: &Path,
) -> Result<Option<Vec<PythonRequest>>, std::io::Error> {
    if let Some(versions) = read_versions_file(directory).await? {
        Ok(Some(
            versions
                .into_iter()
                .map(|version| PythonRequest::parse(&version))
                .collect(),
        ))
    } else if let Some(version) = read_version_file(directory).await? {
        Ok(Some(vec![PythonRequest::parse(&version)]))
    } else {
        Ok(None)
    }
}

/// Read a [`PythonRequest`] from a version file, if present.
///
/// Find the version file inside directory, or the current directory
/// if None.
///
/// Prefers `.python-version` then the first entry of `.python-versions`.
/// If multiple Python versions are desired, use [`requests_from_version_files`] instead.
pub async fn request_from_version_file(
    directory: &Path,
) -> Result<Option<PythonRequest>, std::io::Error> {
    if let Some(version) = read_version_file(directory).await? {
        Ok(Some(PythonRequest::parse(&version)))
    } else if let Some(versions) = read_versions_file(directory).await? {
        Ok(versions
            .into_iter()
            .next()
            .inspect(|_| debug!("Using the first version from `{PYTHON_VERSIONS_FILENAME}`"))
            .map(|version| PythonRequest::parse(&version)))
    } else {
        Ok(None)
    }
}

/// Write a version to a .`python-version` file.
pub async fn write_version_file(version: &str) -> Result<(), std::io::Error> {
    debug!("Writing Python version `{version}` to `{PYTHON_VERSION_FILENAME}`");
    fs::tokio::write(PYTHON_VERSION_FILENAME, format!("{version}\n")).await
}

async fn read_versions_file(directory: &Path) -> Result<Option<Vec<String>>, std::io::Error> {
    let path = directory.join(PYTHON_VERSIONS_FILENAME);
    match fs::tokio::read_to_string(&path).await {
        Ok(content) => {
            debug!("Reading requests from `{}`", path.display());
            Ok(Some(
                content
                    .lines()
                    .filter(|line| {
                        // Skip comments and empty lines.
                        let trimmed = line.trim();
                        !(trimmed.is_empty() || trimmed.starts_with('#'))
                    })
                    .map(ToString::to_string)
                    .collect(),
            ))
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err),
    }
}

async fn read_version_file(directory: &Path) -> Result<Option<String>, std::io::Error> {
    let path = directory.join(PYTHON_VERSION_FILENAME);
    match fs::tokio::read_to_string(&path).await {
        Ok(content) => {
            debug!("Reading requests from `{}`", path.display());
            Ok(content
                .lines()
                .find(|line| {
                    // Skip comments and empty lines.
                    let trimmed = line.trim();
                    !(trimmed.is_empty() || trimmed.starts_with('#'))
                })
                .map(ToString::to_string))
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err),
    }
}
