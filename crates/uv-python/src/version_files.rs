use std::ops::Add;
use std::path::{Path, PathBuf};

use fs_err as fs;
use itertools::Itertools;
use tracing::debug;
use uv_dirs::user_uv_config_dir;
use uv_fs::Simplified;

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

/// Whether to prefer the `.python-version` or `.python-versions` file.
#[derive(Debug, Clone, Copy, Default)]
pub enum FilePreference {
    #[default]
    Version,
    Versions,
}

#[derive(Debug, Default, Clone)]
pub struct DiscoveryOptions<'a> {
    /// The path to stop discovery at.
    stop_discovery_at: Option<&'a Path>,
    /// When `no_config` is set, Python version files will be ignored.
    ///
    /// Discovery will still run in order to display a log about the ignored file.
    no_config: bool,
    preference: FilePreference,
}

impl<'a> DiscoveryOptions<'a> {
    #[must_use]
    pub fn with_no_config(self, no_config: bool) -> Self {
        Self { no_config, ..self }
    }

    #[must_use]
    pub fn with_preference(self, preference: FilePreference) -> Self {
        Self { preference, ..self }
    }

    #[must_use]
    pub fn with_stop_discovery_at(self, stop_discovery_at: Option<&'a Path>) -> Self {
        Self {
            stop_discovery_at,
            ..self
        }
    }
}

impl PythonVersionFile {
    /// Find a Python version file in the given directory or any of its parents.
    pub async fn discover(
        working_directory: impl AsRef<Path>,
        options: &DiscoveryOptions<'_>,
    ) -> Result<Option<Self>, std::io::Error> {
        let Some(path) = Self::find_nearest(working_directory, options) else {
            // Not found in directory or its ancestors. Looking in user-level config.
            return Ok(match user_uv_config_dir() {
                Some(user_dir) => Self::discover_user_config(user_dir, options)
                    .await?
                    .or(None),
                None => None,
            });
        };

        if options.no_config {
            debug!(
                "Ignoring Python version file at `{}` due to `--no-config`",
                path.user_display()
            );
            return Ok(None);
        }

        // Uses `try_from_path` instead of `from_path` to avoid TOCTOU failures.
        Self::try_from_path(path).await
    }

    pub async fn discover_user_config(
        user_config_working_directory: impl AsRef<Path>,
        options: &DiscoveryOptions<'_>,
    ) -> Result<Option<Self>, std::io::Error> {
        if !options.no_config {
            if let Some(path) =
                Self::find_in_directory(user_config_working_directory.as_ref(), options)
                    .into_iter()
                    .find(|path| path.is_file())
            {
                return Self::try_from_path(path).await;
            }
        }
        Ok(None)
    }

    fn find_nearest(path: impl AsRef<Path>, options: &DiscoveryOptions<'_>) -> Option<PathBuf> {
        path.as_ref()
            .ancestors()
            .take_while(|path| {
                // Only walk up the given directory, if any.
                options
                    .stop_discovery_at
                    .and_then(Path::parent)
                    .map(|stop_discovery_at| stop_discovery_at != *path)
                    .unwrap_or(true)
            })
            .find_map(|path| Self::find_in_directory(path, options))
    }

    fn find_in_directory(path: &Path, options: &DiscoveryOptions<'_>) -> Option<PathBuf> {
        let version_path = path.join(PYTHON_VERSION_FILENAME);
        let versions_path = path.join(PYTHON_VERSIONS_FILENAME);

        let paths = match options.preference {
            FilePreference::Versions => [versions_path, version_path],
            FilePreference::Version => [version_path, versions_path],
        };

        paths.into_iter().find(|path| path.is_file())
    }

    /// Try to read a Python version file at the given path.
    ///
    /// If the file does not exist, `Ok(None)` is returned.
    pub async fn try_from_path(path: PathBuf) -> Result<Option<Self>, std::io::Error> {
        match fs::tokio::read_to_string(&path).await {
            Ok(content) => {
                debug!(
                    "Reading Python requests from version file at `{}`",
                    path.display()
                );
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

    /// Return the first request declared in the file, if any.
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
