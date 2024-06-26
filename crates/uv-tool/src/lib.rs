use core::fmt;
use fs_err as fs;
use install_wheel_rs::linker::entrypoint_path;
use install_wheel_rs::{scripts_from_ini, Script};
use pep440_rs::Version;
use pep508_rs::PackageName;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use thiserror::Error;
use tracing::debug;
use uv_cache::Cache;
use uv_fs::{LockedFile, Simplified};
use uv_toolchain::{Interpreter, PythonEnvironment};

pub use receipt::ToolReceipt;
pub use tool::Tool;

use uv_state::{StateBucket, StateStore};
mod receipt;
mod tool;

#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    IO(#[from] io::Error),
    #[error("Failed to update `uv-receipt.toml` at {0}")]
    ReceiptWrite(PathBuf, #[source] Box<toml::ser::Error>),
    #[error("Failed to read `uv-receipt.toml` at {0}")]
    ReceiptRead(PathBuf, #[source] Box<toml::de::Error>),
    #[error(transparent)]
    VirtualEnvError(#[from] uv_virtualenv::Error),
    #[error("Failed to read package entry points {0}")]
    EntrypointRead(#[from] install_wheel_rs::Error),
    #[error("Failed to find dist-info directory `{0}` in environment at {1}")]
    DistInfoMissing(String, PathBuf),
    #[error("Failed to find a directory for executables")]
    NoExecutableDirectory,
    #[error(transparent)]
    EnvironmentError(#[from] uv_toolchain::Error),
    #[error("Failed to find a receipt for tool `{0}` at {1}")]
    MissingToolReceipt(String, PathBuf),
}

/// A collection of uv-managed tools installed on the current system.
#[derive(Debug, Clone)]
pub struct InstalledTools {
    /// The path to the top-level directory of the tools.
    root: PathBuf,
}

impl InstalledTools {
    /// A directory for tools at `root`.
    fn from_path(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Create a new [`InstalledTools`] from settings.
    ///
    /// Prefer, in order:
    ///
    /// 1. The specific tool directory specified by the user, i.e., `UV_TOOL_DIR`
    /// 2. A directory in the system-appropriate user-level data directory, e.g., `~/.local/uv/tools`
    /// 3. A directory in the local data directory, e.g., `./.uv/tools`
    pub fn from_settings() -> Result<Self, Error> {
        if let Some(tool_dir) = std::env::var_os("UV_TOOL_DIR") {
            Ok(Self::from_path(tool_dir))
        } else {
            Ok(Self::from_path(
                StateStore::from_settings(None)?.bucket(StateBucket::Tools),
            ))
        }
    }

    /// Return the metadata for all installed tools.
    pub fn tools(&self) -> Result<Vec<(String, Tool)>, Error> {
        let _lock = self.acquire_lock();
        let mut tools = Vec::new();
        for directory in uv_fs::directories(self.root()) {
            let name = directory.file_name().unwrap().to_string_lossy().to_string();
            let path = directory.join("uv-receipt.toml");
            let contents = match fs_err::read_to_string(&path) {
                Ok(contents) => contents,
                // TODO(zanieb): Consider warning on malformed tools instead
                Err(err) if err.kind() == io::ErrorKind::NotFound => {
                    return Err(Error::MissingToolReceipt(name.clone(), path.clone()))
                }
                Err(err) => return Err(err.into()),
            };
            let tool_receipt = ToolReceipt::from_string(contents)
                .map_err(|err| Error::ReceiptRead(path, Box::new(err)))?;
            tools.push((name, tool_receipt.tool));
        }
        Ok(tools)
    }

    /// Get the receipt for the given tool.
    pub fn get_tool_receipt(&self, name: &str) -> Result<Option<Tool>, Error> {
        let path = self.root.join(name).join("uv-receipt.toml");
        match ToolReceipt::from_path(&path) {
            Ok(tool_receipt) => Ok(Some(tool_receipt.tool)),
            Err(Error::IO(err)) if err.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(err),
        }
    }

    /// Lock the tools directory.
    fn acquire_lock(&self) -> Result<LockedFile, Error> {
        Ok(LockedFile::acquire(
            self.root.join(".lock"),
            self.root.user_display(),
        )?)
    }

    /// Lock a tool directory.
    fn acquire_tool_lock(&self, name: &str) -> Result<LockedFile, Error> {
        let path = self.root.join(name);
        Ok(LockedFile::acquire(
            path.join(".lock"),
            path.user_display(),
        )?)
    }

    /// Add a receipt for a tool.
    ///
    /// Any existing receipt will be replaced.
    pub fn add_tool_receipt(&self, name: &str, tool: Tool) -> Result<(), Error> {
        let _lock = self.acquire_tool_lock(name);

        let tool_receipt = ToolReceipt::from(tool);
        let path = self.root.join(name).join("uv-receipt.toml");

        debug!(
            "Adding metadata entry for tool `{name}` at {}",
            path.user_display()
        );

        let doc = toml::to_string(&tool_receipt)
            .map_err(|err| Error::ReceiptWrite(path.clone(), Box::new(err)))?;

        // Save the modified `tools.toml`.
        fs_err::write(&path, doc)?;

        Ok(())
    }

    pub fn remove_environment(&self, name: &str) -> Result<(), Error> {
        let _lock = self.acquire_lock();
        let environment_path = self.root.join(name);

        debug!(
            "Deleting environment for tool `{name}` at {}",
            environment_path.user_display()
        );

        fs_err::remove_dir_all(environment_path)?;

        Ok(())
    }

    pub fn environment(
        &self,
        name: &str,
        remove_existing: bool,
        interpreter: Interpreter,
        cache: &Cache,
    ) -> Result<PythonEnvironment, Error> {
        let _lock = self.acquire_lock();
        let environment_path = self.root.join(name);

        if !remove_existing && environment_path.exists() {
            debug!(
                "Using existing environment for tool `{name}` at `{}`.",
                environment_path.user_display()
            );
            return Ok(PythonEnvironment::from_root(environment_path, cache)?);
        }

        debug!(
            "Creating environment for tool `{name}` at `{}`.",
            environment_path.user_display()
        );

        // Create a virtual environment.
        let venv = uv_virtualenv::create_venv(
            &environment_path,
            interpreter,
            uv_virtualenv::Prompt::None,
            false,
            false,
        )?;

        Ok(venv)
    }

    /// Create a temporary tools directory.
    pub fn temp() -> Result<Self, Error> {
        Ok(Self::from_path(
            StateStore::temp()?.bucket(StateBucket::Tools),
        ))
    }

    /// Initialize the tools directory.
    ///
    /// Ensures the directory is created.
    pub fn init(self) -> Result<Self, Error> {
        let root = &self.root;

        // Create the tools directory, if it doesn't exist.
        fs::create_dir_all(root)?;

        // Add a .gitignore.
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(root.join(".gitignore"))
        {
            Ok(mut file) => file.write_all(b"*")?,
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => (),
            Err(err) => return Err(err.into()),
        }

        Ok(self)
    }

    /// Return the path of the tools directory.
    pub fn root(&self) -> &Path {
        &self.root
    }
}

/// A uv-managed tool installed on the current system..
#[derive(Debug, Clone)]
pub struct InstalledTool {
    /// The path to the top-level directory of the tools.
    path: PathBuf,
}

impl InstalledTool {
    pub fn new(path: PathBuf) -> Result<Self, Error> {
        Ok(Self { path })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl fmt::Display for InstalledTool {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            self.path
                .file_name()
                .unwrap_or(self.path.as_os_str())
                .to_string_lossy()
        )
    }
}

/// Find a directory to place executables in.
///
/// This follows, in order:
///
/// - `$XDG_BIN_HOME`
/// - `$XDG_DATA_HOME/../bin`
/// - `$HOME/.local/bin`
///
/// On all platforms.
///
/// Errors if a directory cannot be found.
pub fn find_executable_directory() -> Result<PathBuf, Error> {
    std::env::var_os("XDG_BIN_HOME")
        .and_then(dirs_sys::is_absolute_path)
        .or_else(|| {
            std::env::var_os("XDG_DATA_HOME")
                .and_then(dirs_sys::is_absolute_path)
                .map(|path| path.join("../bin"))
        })
        .or_else(|| {
            // See https://github.com/dirs-dev/dirs-rs/blob/50b50f31f3363b7656e5e63b3fa1060217cbc844/src/win.rs#L5C58-L5C78
            #[cfg(windows)]
            let home_dir = dirs_sys::known_folder_profile();
            #[cfg(not(windows))]
            let home_dir = dirs_sys::home_dir();
            home_dir.map(|path| path.join(".local").join("bin"))
        })
        .ok_or(Error::NoExecutableDirectory)
}

/// Find the dist-info directory for a package in an environment.
fn find_dist_info(
    environment: &PythonEnvironment,
    package_name: &PackageName,
    package_version: &Version,
) -> Result<PathBuf, Error> {
    let dist_info_prefix = format!("{package_name}-{package_version}.dist-info");
    environment
        .interpreter()
        .site_packages()
        .map(|path| path.join(&dist_info_prefix))
        .find(|path| path.exists())
        .ok_or_else(|| Error::DistInfoMissing(dist_info_prefix, environment.root().to_path_buf()))
}

/// Parses the `entry_points.txt` entry for console scripts
///
/// Returns (`script_name`, module, function)
fn parse_scripts(
    dist_info_path: &Path,
    python_minor: u8,
) -> Result<(Vec<Script>, Vec<Script>), Error> {
    let entry_points_path = dist_info_path.join("entry_points.txt");

    // Read the entry points mapping. If the file doesn't exist, we just return an empty mapping.
    let Ok(ini) = fs::read_to_string(&entry_points_path) else {
        debug!(
            "Failed to read entry points at {}",
            entry_points_path.user_display()
        );
        return Ok((Vec::new(), Vec::new()));
    };

    Ok(scripts_from_ini(None, python_minor, ini)?)
}

/// Find the paths to the entry points provided by a package in an environment.
///
/// Returns a list of `(name, path)` tuples.
pub fn entrypoint_paths(
    environment: &PythonEnvironment,
    package_name: &PackageName,
    package_version: &Version,
) -> Result<Vec<(String, PathBuf)>, Error> {
    let dist_info_path = find_dist_info(environment, package_name, package_version)?;
    debug!("Looking at dist-info at {}", dist_info_path.user_display());

    let (console_scripts, gui_scripts) =
        parse_scripts(&dist_info_path, environment.interpreter().python_minor())?;

    let layout = environment.interpreter().layout();

    Ok(console_scripts
        .into_iter()
        .chain(gui_scripts)
        .map(|entrypoint| {
            let path = entrypoint_path(&entrypoint, &layout);
            (entrypoint.name, path)
        })
        .collect())
}
