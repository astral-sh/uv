use std::ops::Deref;
use std::path::{Path, PathBuf};

use tracing::debug;

use uv_fs::Simplified;
#[cfg(not(windows))]
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

        debug!("Searching for user configuration in: `{}`", file.display());
        match read_file(&file) {
            Ok(options) => {
                debug!("Found user configuration in: `{}`", file.display());
                Ok(Some(Self(options)))
            }
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

    pub fn system() -> Result<Option<Self>, Error> {
        let Some(file) = system_config_file() else {
            return Ok(None);
        };
        debug!("Found system configuration in: `{}`", file.display());
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

                debug!("Found workspace configuration at `{}`", path.display());
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
                    debug!(
                        "Skipping `pyproject.toml` in `{}` (no `[tool]` section)",
                        dir.display()
                    );
                    return Ok(None);
                };
                let Some(options) = tool.uv else {
                    debug!(
                        "Skipping `pyproject.toml` in `{}` (no `[tool.uv]` section)",
                        dir.display()
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

impl From<Options> for FilesystemOptions {
    fn from(options: Options) -> Self {
        Self(options)
    }
}

/// Returns the path to the user configuration directory.
///
/// This is similar to the `config_dir()` returned by the `dirs` crate, but it uses the
/// `XDG_CONFIG_HOME` environment variable on both Linux _and_ macOS, rather than the
/// `Application Support` directory on macOS.
fn user_config_dir() -> Option<PathBuf> {
    // On Windows, use, e.g., C:\Users\Alice\AppData\Roaming
    #[cfg(windows)]
    {
        dirs_sys::known_folder_roaming_app_data()
    }

    // On Linux and macOS, use, e.g., /home/alice/.config.
    #[cfg(not(windows))]
    {
        std::env::var_os(EnvVars::XDG_CONFIG_HOME)
            .and_then(dirs_sys::is_absolute_path)
            .or_else(|| dirs_sys::home_dir().map(|path| path.join(".config")))
    }
}

fn locate_system_config_xdg() -> Option<PathBuf> {
    // On Linux/MacOS systems, read the XDG_CONFIG_DIRS environment variable
    if let Ok(config_dirs) = std::env::var("XDG_CONFIG_DIRS") {
        for dir in config_dirs.split(':').take_while(|s| !s.is_empty()) {
            let uv_toml_path = Path::new(dir).join("uv").join("uv.toml");

            if uv_toml_path.is_file() {
                return Some(uv_toml_path);
            }
        }
    }
    None
}

/// Returns the path to the system configuration file.
///
/// Unix-like systems: This uses the `XDG_CONFIG_DIRS` environment variable in *nix systems.
/// If the var is not present it will check /etc/xdg/uv/uv.toml and then /etc/uv/uv.toml.
/// Windows: "C:\ProgramData\uv\uv.toml" is used.
fn system_config_file() -> Option<PathBuf> {
    // On Windows, use, e.g., C:\ProgramData
    #[cfg(windows)]
    {
        PathBuf::from("C:\\ProgramData\\uv\\uv.toml")
    }

    #[cfg(not(windows))]
    {
        if let Some(path) = locate_system_config_xdg() {
            return Some(path);
        }
        // Fallback to /etc/xdg/uv/uv.toml then /etc/uv/uv.toml if XDG_CONFIG_DIRS is not set or no
        // valid path is found
        for candidate in ["/etc/xdg/uv/uv.toml", "/etc/uv/uv.toml"] {
            if Path::new(candidate).is_file() {
                return Some(PathBuf::from(candidate));
            }
        }
        None
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
    use crate::locate_system_config_xdg;
    use std::env;
    use std::path::Path;

    #[cfg(not(windows))]
    #[test]
    fn test_locate_system_config_xdg() {
        // Construct the path to the uv.toml file in the tests/fixtures directory
        let td = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures");
        let tf = td.join("uv").join("uv.toml");

        let cur_dir = env::current_dir().unwrap();
        {
            env::set_current_dir(&td).unwrap();

            // Empty string
            env::set_var("XDG_CONFIG_DIRS", "");
            assert_eq!(locate_system_config_xdg(), None);

            // Single colon
            env::set_var("XDG_CONFIG_DIRS", ":");
            assert_eq!(locate_system_config_xdg(), None);
        }

        env::set_current_dir(&cur_dir).unwrap();
        assert_eq!(locate_system_config_xdg(), None);

        // Set the environment variable to the directory containing the uv.toml file
        env::set_var("XDG_CONFIG_DIRS", td.clone());

        // Assert that the system_config_file function returns the correct path
        assert_eq!(locate_system_config_xdg().unwrap(), tf);

        let first_td = td.join("first");
        let first_tf = first_td.join("uv").join("uv.toml");
        env::set_var(
            "XDG_CONFIG_DIRS",
            format!("{}:{}", first_td.to_string_lossy(), td.to_string_lossy()),
        );
        assert_eq!(locate_system_config_xdg().unwrap(), first_tf);
    }
}
