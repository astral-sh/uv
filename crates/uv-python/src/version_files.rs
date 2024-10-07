use std::ops::Add;
use std::path::{Path, PathBuf};

use fs_err as fs;
use itertools::Itertools;
use tracing::debug;

use crate::PythonRequest;

/// The file name for Python version pins.
pub static PYTHON_VERSION_FILENAME: &str = ".python-version";

/// The file name for multiple Python version declarations.
pub static PYTHON_VERSIONS_FILENAME: &str = ".python-versions";

/// A `.python-version` or `.python-versions` file.
#[derive(Debug, Clone)]
pub struct PythonVersionFile {
    /// The path to the version file.
    path: PathBuf,
    /// The Python version requests declared in the file.
    versions: Vec<PythonRequest>,
}

impl PythonVersionFile {
    /// Find a Python version file in the given directory.
    pub async fn discover(
        working_directory: impl AsRef<Path>,
        // TODO(zanieb): Create a `DiscoverySettings` struct for these options
        no_config: bool,
        prefer_versions: bool,
    ) -> Result<Option<Self>, std::io::Error> {
        let versions_path = working_directory.as_ref().join(PYTHON_VERSIONS_FILENAME);
        let version_path = working_directory.as_ref().join(PYTHON_VERSION_FILENAME);

        if no_config {
            if version_path.exists() {
                debug!("Ignoring `.python-version` file due to `--no-config`");
            } else if versions_path.exists() {
                debug!("Ignoring `.python-versions` file due to `--no-config`");
            };
            return Ok(None);
        }

        let paths = if prefer_versions {
            [versions_path, version_path]
        } else {
            [version_path, versions_path]
        };
        for path in paths {
            if let Some(result) = Self::try_from_path(path).await? {
                return Ok(Some(result));
            };
        }

        Ok(None)
    }

    /// Try to read a Python version file at the given path.
    ///
    /// If the file does not exist, `Ok(None)` is returned.
    pub async fn try_from_path(path: PathBuf) -> Result<Option<Self>, std::io::Error> {
        match fs::tokio::read_to_string(&path).await {
            Ok(content) => {
                debug!("Reading requests from `{}`", path.display());
                let versions = content
                    .lines()
                    .filter(|line| {
                        // Skip comments and empty lines.
                        let trimmed = line.trim();
                        !(trimmed.is_empty() || trimmed.starts_with('#'))
                    })
                    .map(ToString::to_string)
                    .map(|version| PythonRequest::parse(&version))
                    .collect();
                Ok(Some(Self { path, versions }))
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(err),
        }
    }

    /// Read a Python version file at the given path.
    ///
    /// If the file does not exist, an error is returned.
    pub async fn from_path(path: PathBuf) -> Result<Self, std::io::Error> {
        let Some(result) = Self::try_from_path(path).await? else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "Version file not found".to_string(),
            ));
        };
        Ok(result)
    }

    /// Create a new representation of a version file at the given path.
    ///
    /// The file will not any include versions; see [`PythonVersionFile::with_versions`].
    /// The file will not be created; see [`PythonVersionFile::write`].
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            versions: vec![],
        }
    }

    /// Return the first version declared in the file, if any.
    pub fn version(&self) -> Option<&PythonRequest> {
        self.versions.first()
    }

    /// Iterate of all versions declared in the file.
    pub fn versions(&self) -> impl Iterator<Item = &PythonRequest> {
        self.versions.iter()
    }

    /// Cast to a list of all versions declared in the file.
    pub fn into_versions(self) -> Vec<PythonRequest> {
        self.versions
    }

    /// Cast to the first version declared in the file, if any.
    pub fn into_version(self) -> Option<PythonRequest> {
        self.versions.into_iter().next()
    }

    /// Return the path to the version file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Return the file name of the version file (guaranteed to be one of `.python-version` or
    /// `.python-versions`).
    pub fn file_name(&self) -> &str {
        self.path.file_name().unwrap().to_str().unwrap()
    }

    /// Set the versions for the file.
    #[must_use]
    pub fn with_versions(self, versions: Vec<PythonRequest>) -> Self {
        Self {
            path: self.path,
            versions,
        }
    }

    /// Update the version file on the file system.
    pub async fn write(&self) -> Result<(), std::io::Error> {
        debug!("Writing Python versions to `{}`", self.path.display());
        fs::tokio::write(
            &self.path,
            self.versions
                .iter()
                .map(PythonRequest::to_canonical_string)
                .join("\n")
                .add("\n")
                .as_bytes(),
        )
        .await
    }
}
