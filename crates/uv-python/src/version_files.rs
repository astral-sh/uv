use fs_err as fs;
use tracing::debug;
use uv_fs::Simplified;

use crate::PythonRequest;

/// The file name for Python version pins.
pub static PYTHON_VERSION_FILENAME: &str = ".python-version";

/// The file name for multiple Python version declarations.
pub static PYTHON_VERSIONS_FILENAME: &str = ".python-versions";

/// Read [`PythonRequest`]s from a version file in the given directory, if present.
///
/// Prefers `.python-versions` then `.python-version`.
/// If only one Python version is desired, use [`request_from_version_files`] which prefers the `.python-version` file.
pub async fn requests_from_version_file_in(
    parent: impl AsRef<std::path::Path>,
) -> Result<Option<Vec<PythonRequest>>, std::io::Error> {
    if let Some(versions) =
        read_versions_file(&parent.as_ref().join(PYTHON_VERSIONS_FILENAME)).await?
    {
        Ok(Some(
            versions
                .into_iter()
                .map(|version| PythonRequest::parse(&version))
                .collect(),
        ))
    } else if let Some(version) =
        read_version_file(&parent.as_ref().join(PYTHON_VERSION_FILENAME)).await?
    {
        Ok(Some(vec![PythonRequest::parse(&version)]))
    } else {
        Ok(None)
    }
}

/// Read a [`PythonRequest`] from a version file in the given directory, if present.
///
/// Prefers `.python-version` then the first entry of `.python-versions`.
/// If multiple Python versions are desired, use [`requests_from_version_files`] instead.
pub async fn request_from_version_file_in(
    parent: impl AsRef<std::path::Path>,
) -> Result<Option<PythonRequest>, std::io::Error> {
    if let Some(version) = read_version_file(&parent.as_ref().join(PYTHON_VERSION_FILENAME)).await?
    {
        Ok(Some(PythonRequest::parse(&version)))
    } else if let Some(versions) =
        read_versions_file(&parent.as_ref().join(PYTHON_VERSIONS_FILENAME)).await?
    {
        Ok(versions
            .into_iter()
            .next()
            .inspect(|_| debug!("Using the first version from `{PYTHON_VERSIONS_FILENAME}`"))
            .map(|version| PythonRequest::parse(&version)))
    } else {
        Ok(None)
    }
}

/// Write a version to a `.python-version`-formatted file.
pub async fn write_version_file(
    path: impl AsRef<std::path::Path>,
    version: &str,
) -> Result<(), std::io::Error> {
    debug!(
        "Writing Python version `{version}` to `{}`",
        path.user_display()
    );
    fs::tokio::write(path, format!("{version}\n")).await
}

/// Read versions from a `.python-versions`-formatted file.
async fn read_versions_file(
    path: impl AsRef<std::path::Path>,
) -> Result<Option<Vec<String>>, std::io::Error> {
    match fs::tokio::read_to_string(&path).await {
        Ok(content) => {
            debug!("Reading requests from `{}`", path.user_display());
            Ok(Some(content.lines().map(ToString::to_string).collect()))
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err),
    }
}

/// Read a version from a `.python-version`-formatted file.
async fn read_version_file(
    path: impl AsRef<std::path::Path>,
) -> Result<Option<String>, std::io::Error> {
    match fs::tokio::read_to_string(&path).await {
        Ok(content) => {
            debug!("Reading requests from `{}`", path.user_display());
            Ok(content.lines().next().map(ToString::to_string))
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err),
    }
}
