use std::ops::Deref;
use std::path::{Path, PathBuf};

use tracing::debug;

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
        let Some(dir) = config_dir() else {
            return Ok(None);
        };
        let root = dir.join("uv");
        let file = root.join("uv.toml");

        debug!("Loading user configuration from: `{}`", file.display());
        match read_file(&file) {
            Ok(options) => Ok(Some(Self(options))),
            Err(Error::Io(err)) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(_) if !dir.is_dir() => {
                // Ex) `XDG_CONFIG_HOME=/dev/null`
                debug!(
                    "User configuration directory `{}` does not exist or is not a directory",
                    dir.display()
                );
                Ok(None)
            }
            Err(err) => Err(err),
        }
    }

    /// Find the [`FilesystemOptions`] for the given path.
    ///
    /// The search starts at the given path and goes up the directory tree until a `uv.toml` file or
    /// `pyproject.toml` file is found.
    pub fn find(path: impl AsRef<Path>) -> Result<Option<Self>, Error> {
        for ancestor in path.as_ref().ancestors() {
            match Self::from_directory(ancestor) {
                Ok(Some(options)) => {
                    return Ok(Some(options));
                }
                Ok(None) => {
                    // Continue traversing the directory tree.
                }
                Err(Error::PyprojectToml(file, _err)) => {
                    // If we see an invalid `pyproject.toml`, warn but continue.
                    warn_user!("Failed to parse `{file}` during settings discovery; skipping...");
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
    pub fn from_directory(dir: impl AsRef<Path>) -> Result<Option<Self>, Error> {
        // Read a `uv.toml` file in the current directory.
        let path = dir.as_ref().join("uv.toml");
        match fs_err::read_to_string(&path) {
            Ok(content) => {
                let options: Options = toml::from_str(&content)
                    .map_err(|err| Error::UvToml(path.user_display().to_string(), err))?;

                // If the directory also contains a `[tool.uv]` table in a `pyproject.toml` file,
                // warn.
                let pyproject = dir.as_ref().join("pyproject.toml");
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

                debug!("Found workspace configuration at `{}`", path.display());
                return Ok(Some(Self(options)));
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => return Err(err.into()),
        }

        // Read a `pyproject.toml` file in the current directory.
        let path = dir.as_ref().join("pyproject.toml");
        match fs_err::read_to_string(&path) {
            Ok(content) => {
                // Parse, but skip any `pyproject.toml` that doesn't have a `[tool.uv]` section.
                let pyproject: PyProjectToml = toml::from_str(&content)
                    .map_err(|err| Error::PyprojectToml(path.user_display().to_string(), err))?;
                let Some(tool) = pyproject.tool else {
                    debug!(
                        "Skipping `pyproject.toml` in `{}` (no `[tool]` section)",
                        dir.as_ref().display()
                    );
                    return Ok(None);
                };
                let Some(options) = tool.uv else {
                    debug!(
                        "Skipping `pyproject.toml` in `{}` (no `[tool.uv]` section)",
                        dir.as_ref().display()
                    );
                    return Ok(None);
                };

                debug!("Found workspace configuration at `{}`", path.display());
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

/// Returns the path to the user configuration directory.
///
/// This is similar to the `config_dir()` returned by the `dirs` crate, but it uses the
/// `XDG_CONFIG_HOME` environment variable on both Linux _and_ macOS, rather than the
/// `Application Support` directory on macOS.
fn config_dir() -> Option<PathBuf> {
    // On Windows, use, e.g., C:\Users\Alice\AppData\Roaming
    #[cfg(windows)]
    {
        dirs_sys::known_folder_roaming_app_data()
    }

    // On Linux and macOS, use, e.g., /home/alice/.config.
    #[cfg(not(windows))]
    {
        std::env::var_os("XDG_CONFIG_HOME")
            .and_then(dirs_sys::is_absolute_path)
            .or_else(|| dirs_sys::home_dir().map(|path| path.join(".config")))
    }
}

/// Load [`Options`] from a `uv.toml` file.
fn read_file(path: &Path) -> Result<Options, Error> {
    let content = fs_err::read_to_string(path)?;
    let options: Options = toml::from_str(&content)
        .map_err(|err| Error::UvToml(path.user_display().to_string(), err))?;
    Ok(options)
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error("Failed to parse: `{0}`")]
    PyprojectToml(String, #[source] toml::de::Error),

    #[error("Failed to parse: `{0}`")]
    UvToml(String, #[source] toml::de::Error),
}
