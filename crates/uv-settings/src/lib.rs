use std::num::NonZeroUsize;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::Duration;
use tracing::info_span;
use uv_client::{DEFAULT_CONNECT_TIMEOUT, DEFAULT_READ_TIMEOUT, DEFAULT_READ_TIMEOUT_UPLOAD};
use uv_configuration::{RequiredVersion, ResolutionPolicy};
use uv_dirs::{system_config_file, user_config_dir};
use uv_distribution_types::Origin;
use uv_flags::EnvironmentFlags;
use uv_fs::Simplified;
use uv_normalize::{GroupName, PackageName};
use uv_pep440::Version;
use uv_redacted::DisplaySafeUrl;
use uv_static::{EnvVars, InvalidEnvironmentVariable, parse_boolish_environment_variable};
use uv_torch::AmdGpuArchitecture;
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
                Ok(Some(Self(options.with_origin(Origin::User))))
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
        if parse_boolish_environment_variable(EnvVars::UV_NO_SYSTEM_CONFIG)? == Some(true) {
            return Ok(None);
        }

        let Some(file) = system_config_file() else {
            return Ok(None);
        };

        tracing::debug!("Found system configuration in: `{}`", file.display());
        let options = read_file(&file)?;
        validate_uv_toml(&file, &options)?;
        Ok(Some(Self(options.with_origin(Origin::System))))
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
    fn from_directory(dir: &Path) -> Result<Option<Self>, Error> {
        // Read a `uv.toml` file in the current directory.
        let path = dir.join("uv.toml");
        match fs_err::read_to_string(&path) {
            Ok(content) => {
                let options =
                    info_span!("toml::from_str filesystem options uv.toml", path = %path.display())
                        .in_scope(|| toml::from_str::<Options>(&content))
                        .map_err(|err| {
                            check_uv_toml_required_version(
                                &path,
                                &content,
                                Error::UvToml(path.clone(), Box::new(err)),
                            )
                        })?
                        .relative_to(&std::path::absolute(dir)?)?;

                // If the directory also contains a `[tool.uv]` table in a `pyproject.toml` file,
                // warn.
                let pyproject = dir.join("pyproject.toml");
                if let Ok(content) = fs_err::read_to_string(&pyproject) {
                    let result = info_span!("toml::from_str filesystem options pyproject.toml", path = %pyproject.display())
                        .in_scope(|| toml::from_str::<PyProjectToml>(&content)).ok();
                    if let Some(options) =
                        result.and_then(|pyproject| pyproject.tool.and_then(|tool| tool.uv))
                    {
                        warn_uv_toml_masked_fields(&options);
                    }
                }

                tracing::debug!("Found workspace configuration at `{}`", path.display());
                validate_uv_toml(&path, &options)?;
                return Ok(Some(Self(options.with_origin(Origin::Project))));
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => return Err(err.into()),
        }

        // Read a `pyproject.toml` file in the current directory.
        let path = dir.join("pyproject.toml");
        match fs_err::read_to_string(&path) {
            Ok(content) => {
                // Parse, but skip any `pyproject.toml` that doesn't have a `[tool.uv]` section.
                let pyproject =
                    info_span!("toml::from_str filesystem options pyproject.toml", path = %path.display())
                        .in_scope(|| toml::from_str::<PyProjectToml>(&content))
                        .map_err(|err| {
                            check_pyproject_required_version(&path, &content, err)
                        })?;
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
        let path = path.as_ref();
        tracing::debug!("Reading user configuration from: `{}`", path.display());

        let options = read_file(path)?;
        validate_uv_toml(path, &options)?;
        Ok(Self(options))
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
    let options = info_span!("toml::from_str filesystem options uv.toml", path = %path.display())
        .in_scope(|| toml::from_str::<Options>(&content))
        .map_err(|err| {
            check_uv_toml_required_version(
                path,
                &content,
                Error::UvToml(path.to_path_buf(), Box::new(err)),
            )
        })?;
    let options = if let Some(parent) = std::path::absolute(path)?.parent() {
        options.relative_to(parent)?
    } else {
        options
    };
    Ok(options)
}

// ... unchanged code below ...
