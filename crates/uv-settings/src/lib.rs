use std::env;
use std::ops::Deref;
use std::path::{Path, PathBuf};

use etcetera::BaseStrategy;

use uv_fs::Simplified;
use uv_static::EnvVars;
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
                Ok(Some(Self(options)))
            }
            Err(Error::Io(err)) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(_) if !dir.is_dir() => {
                // Ex) `XDG_CONFIG_HOME=/dev/null`
                tracing::debug!(
                    "User configuration directory `{}` does not exist or is not a directory",
                    dir.display()
                );
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
                Err(Error::PyprojectToml(file, err)) => {
                    // If we see an invalid `pyproject.toml`, warn but continue.
                    warn_user!(
                        "Failed to parse `{}` during settings discovery:\n{}",
                        file.cyan(),
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
                let options: Options = toml::from_str(&content)
                    .map_err(|err| Error::UvToml(path.user_display().to_string(), err))?;

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
                    .map_err(|err| Error::PyprojectToml(path.user_display().to_string(), err))?;
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

/// Returns the path to the user configuration directory.
///
/// On Windows, use, e.g., C:\Users\Alice\AppData\Roaming
/// On Linux and macOS, use `XDG_CONFIG_HOME` or $HOME/.config, e.g., /home/alice/.config.
fn user_config_dir() -> Option<PathBuf> {
    etcetera::choose_base_strategy()
        .map(|dirs| dirs.config_dir())
        .ok()
}

#[cfg(not(windows))]
fn locate_system_config_xdg(value: Option<&str>) -> Option<PathBuf> {
    // On Linux and macOS, read the `XDG_CONFIG_DIRS` environment variable.
    let default = "/etc/xdg";
    let config_dirs = value.filter(|s| !s.is_empty()).unwrap_or(default);

    for dir in config_dirs.split(':').take_while(|s| !s.is_empty()) {
        let uv_toml_path = Path::new(dir).join("uv").join("uv.toml");
        if uv_toml_path.is_file() {
            return Some(uv_toml_path);
        }
    }
    None
}

#[cfg(windows)]
fn locate_system_config_windows(system_drive: &std::ffi::OsStr) -> Option<PathBuf> {
    // On Windows, use `%SYSTEMDRIVE%\ProgramData\uv\uv.toml` (e.g., `C:\ProgramData`).
    let candidate = PathBuf::from(system_drive).join("ProgramData\\uv\\uv.toml");
    candidate.as_path().is_file().then_some(candidate)
}

/// Returns the path to the system configuration file.
///
/// On Unix-like systems, uses the `XDG_CONFIG_DIRS` environment variable (falling back to
/// `/etc/xdg/uv/uv.toml` if unset or empty) and then `/etc/uv/uv.toml`
///
/// On Windows, uses `%SYSTEMDRIVE%\ProgramData\uv\uv.toml`.
fn system_config_file() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        env::var_os(EnvVars::SYSTEMDRIVE)
            .and_then(|system_drive| locate_system_config_windows(&system_drive))
    }

    #[cfg(not(windows))]
    {
        if let Some(path) =
            locate_system_config_xdg(env::var(EnvVars::XDG_CONFIG_DIRS).ok().as_deref())
        {
            return Some(path);
        }

        // Fallback to `/etc/uv/uv.toml` if `XDG_CONFIG_DIRS` is not set or no valid
        // path is found.
        let candidate = Path::new("/etc/uv/uv.toml");
        match candidate.try_exists() {
            Ok(true) => Some(candidate.to_path_buf()),
            Ok(false) => None,
            Err(err) => {
                tracing::warn!("Failed to query system configuration file: {err}");
                None
            }
        }
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

#[cfg(test)]
mod test {
    #[cfg(windows)]
    use crate::locate_system_config_windows;
    #[cfg(not(windows))]
    use crate::locate_system_config_xdg;

    use assert_fs::fixture::FixtureError;
    use assert_fs::prelude::*;
    use indoc::indoc;

    #[test]
    #[cfg(not(windows))]
    fn test_locate_system_config_xdg() -> Result<(), FixtureError> {
        // Write a `uv.toml` to a temporary directory.
        let context = assert_fs::TempDir::new()?;
        context.child("uv").child("uv.toml").write_str(indoc! {
            r#"
            [pip]
            index-url = "https://test.pypi.org/simple"
        "#,
        })?;

        // None
        assert_eq!(locate_system_config_xdg(None), None);

        // Empty string
        assert_eq!(locate_system_config_xdg(Some("")), None);

        // Single colon
        assert_eq!(locate_system_config_xdg(Some(":")), None);

        // Assert that the `system_config_file` function returns the correct path.
        assert_eq!(
            locate_system_config_xdg(Some(context.to_str().unwrap())).unwrap(),
            context.child("uv").child("uv.toml").path()
        );

        // Write a separate `uv.toml` to a different directory.
        let first = context.child("first");
        let first_config = first.child("uv").child("uv.toml");
        first_config.write_str("")?;

        assert_eq!(
            locate_system_config_xdg(Some(
                format!("{}:{}", first.to_string_lossy(), context.to_string_lossy()).as_str()
            ))
            .unwrap(),
            first_config.path()
        );

        Ok(())
    }

    #[test]
    #[cfg(windows)]
    fn test_windows_config() -> Result<(), FixtureError> {
        // Write a `uv.toml` to a temporary directory.
        let context = assert_fs::TempDir::new()?;
        context
            .child("ProgramData")
            .child("uv")
            .child("uv.toml")
            .write_str(indoc! { r#"
            [pip]
            index-url = "https://test.pypi.org/simple"
        "#})?;

        // This is typically only a drive (that is, letter and colon) but we
        // allow anything, including a path to the test fixtures...
        assert_eq!(
            locate_system_config_windows(context.path().as_os_str()).unwrap(),
            context
                .child("ProgramData")
                .child("uv")
                .child("uv.toml")
                .path()
        );

        // This does not have a `ProgramData` child, so contains no config.
        let context = assert_fs::TempDir::new()?;
        assert_eq!(
            locate_system_config_windows(context.path().as_os_str()),
            None
        );

        Ok(())
    }
}
