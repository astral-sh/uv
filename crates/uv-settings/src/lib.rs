use std::ops::Deref;
use std::path::{Path, PathBuf};

use uv_dirs::{system_config_file, user_config_dir};
use uv_fs::Simplified;
use uv_warnings::warn_user;

pub use crate::combine::*;
pub use crate::settings::*;

mod combine;
mod settings;

/// The [`Options`] as loaded from a configuration file on disk.
#[derive(Debug, Clone)]
pub struct FilesystemOptions(Options);

impl FilesystemOptions {
    /// Convert the [`FilesystemOptions`] into [`Options`].
    pub fn into_options(self) -> Options {
        self.0
    }
}

impl Deref for FilesystemOptions {
    type Target = Options;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl FilesystemOptions {
    /// Load the user [`FilesystemOptions`].
    pub fn user() -> Result<Option<Self>, Error> {
        let Some(dir) = user_config_dir() else {
            return Ok(None);
        };
        let root = dir.join("uv");
        let file = root.join("uv.toml");

        tracing::debug!("Searching for user configuration in: `{}`", file.display());
        match read_file(&file) {
            Ok(options) => {
                tracing::debug!("Found user configuration in: `{}`", file.display());
                validate_uv_toml(&file, &options)?;
                Ok(Some(Self(options)))
            }
            Err(Error::Io(err))
                if matches!(
                    err.kind(),
                    std::io::ErrorKind::NotFound
                        | std::io::ErrorKind::NotADirectory
                        | std::io::ErrorKind::PermissionDenied
                ) =>
            {
                Ok(None)
            }
            Err(err) => Err(err),
        }
    }

    pub fn system() -> Result<Option<Self>, Error> {
        let Some(file) = system_config_file() else {
            return Ok(None);
        };
        tracing::debug!("Found system configuration in: `{}`", file.display());
        Ok(Some(Self(read_file(&file)?)))
    }

    /// Find the [`FilesystemOptions`] for the given path.
    ///
    /// The search starts at the given path and goes up the directory tree until a `uv.toml` file or
    /// `pyproject.toml` file is found.
    pub fn find(path: &Path) -> Result<Option<Self>, Error> {
        for ancestor in path.ancestors() {
            match Self::from_directory(ancestor) {
                Ok(Some(options)) => {
                    return Ok(Some(options));
                }
                Ok(None) => {
                    // Continue traversing the directory tree.
                }
                Err(Error::PyprojectToml(path, err)) => {
                    // If we see an invalid `pyproject.toml`, warn but continue.
                    warn_user!(
                        "Failed to parse `{}` during settings discovery:\n{}",
                        path.user_display().cyan(),
                        textwrap::indent(&err.to_string(), "  ")
                    );
                }
                Err(err) => {
                    // Otherwise, warn and stop.
                    return Err(err);
                }
            }
        }
        Ok(None)
    }

    /// Load a [`FilesystemOptions`] from a directory, preferring a `uv.toml` file over a
    /// `pyproject.toml` file.
    pub fn from_directory(dir: &Path) -> Result<Option<Self>, Error> {
        // Read a `uv.toml` file in the current directory.
        let path = dir.join("uv.toml");
        match fs_err::read_to_string(&path) {
            Ok(content) => {
                let options = toml::from_str::<Options>(&content)
                    .map_err(|err| Error::UvToml(path.clone(), Box::new(err)))?
                    .relative_to(&std::path::absolute(dir)?)?;

                // If the directory also contains a `[tool.uv]` table in a `pyproject.toml` file,
                // warn.
                let pyproject = dir.join("pyproject.toml");
                if let Some(pyproject) = fs_err::read_to_string(pyproject)
                    .ok()
                    .and_then(|content| toml::from_str::<PyProjectToml>(&content).ok())
                {
                    if pyproject.tool.is_some_and(|tool| tool.uv.is_some()) {
                        warn_user!(
                            "Found both a `uv.toml` file and a `[tool.uv]` section in an adjacent `pyproject.toml`. The `[tool.uv]` section will be ignored in favor of the `uv.toml` file."
                        );
                    }
                }

                tracing::debug!("Found workspace configuration at `{}`", path.display());
                validate_uv_toml(&path, &options)?;
                return Ok(Some(Self(options)));
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => return Err(err.into()),
        }

        // Read a `pyproject.toml` file in the current directory.
        let path = dir.join("pyproject.toml");
        match fs_err::read_to_string(&path) {
            Ok(content) => {
                // Parse, but skip any `pyproject.toml` that doesn't have a `[tool.uv]` section.
                let pyproject: PyProjectToml = toml::from_str(&content)
                    .map_err(|err| Error::PyprojectToml(path.clone(), Box::new(err)))?;
                let Some(tool) = pyproject.tool else {
                    tracing::debug!(
                        "Skipping `pyproject.toml` in `{}` (no `[tool]` section)",
                        dir.display()
                    );
                    return Ok(None);
                };
                let Some(options) = tool.uv else {
                    tracing::debug!(
                        "Skipping `pyproject.toml` in `{}` (no `[tool.uv]` section)",
                        dir.display()
                    );
                    return Ok(None);
                };

                let options = options.relative_to(&std::path::absolute(dir)?)?;

                tracing::debug!("Found workspace configuration at `{}`", path.display());
                return Ok(Some(Self(options)));
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => return Err(err.into()),
        }

        Ok(None)
    }

    /// Load a [`FilesystemOptions`] from a `uv.toml` file.
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, Error> {
        Ok(Self(read_file(path.as_ref())?))
    }
}

impl From<Options> for FilesystemOptions {
    fn from(options: Options) -> Self {
        Self(options)
    }
}

/// Load [`Options`] from a `uv.toml` file.
fn read_file(path: &Path) -> Result<Options, Error> {
    let content = fs_err::read_to_string(path)?;
    let options = toml::from_str::<Options>(&content)
        .map_err(|err| Error::UvToml(path.to_path_buf(), Box::new(err)))?;
    let options = if let Some(parent) = std::path::absolute(path)?.parent() {
        options.relative_to(parent)?
    } else {
        options
    };
    Ok(options)
}

/// Validate that an [`Options`] schema is compatible with `uv.toml`.
fn validate_uv_toml(path: &Path, options: &Options) -> Result<(), Error> {
    // The `uv.toml` format is not allowed to include any of the following, which are
    // permitted by the schema since they _can_ be included in `pyproject.toml` files
    // (and we want to use `deny_unknown_fields`).
    if options.workspace.is_some() {
        return Err(Error::PyprojectOnlyField(path.to_path_buf(), "workspace"));
    }
    if options.sources.is_some() {
        return Err(Error::PyprojectOnlyField(path.to_path_buf(), "sources"));
    }
    if options.dev_dependencies.is_some() {
        return Err(Error::PyprojectOnlyField(
            path.to_path_buf(),
            "dev-dependencies",
        ));
    }
    if options.default_groups.is_some() {
        return Err(Error::PyprojectOnlyField(
            path.to_path_buf(),
            "default-groups",
        ));
    }
    if options.managed.is_some() {
        return Err(Error::PyprojectOnlyField(path.to_path_buf(), "managed"));
    }
    if options.package.is_some() {
        return Err(Error::PyprojectOnlyField(path.to_path_buf(), "package"));
    }
    Ok(())
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Index(#[from] uv_distribution_types::IndexUrlError),

    #[error("Failed to parse: `{}`", _0.user_display())]
    PyprojectToml(PathBuf, #[source] Box<toml::de::Error>),

    #[error("Failed to parse: `{}`", _0.user_display())]
    UvToml(PathBuf, #[source] Box<toml::de::Error>),

    #[error("Failed to parse: `{}`. The `{}` field is not allowed in a `uv.toml` file. `{}` is only applicable in the context of a project, and should be placed in a `pyproject.toml` file instead.", _0.user_display(), _1, _1)]
    PyprojectOnlyField(PathBuf, &'static str),
}
