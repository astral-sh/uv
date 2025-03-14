use core::fmt;
use fs_err as fs;

use uv_dirs::user_executable_directory;
use uv_pep440::Version;
use uv_pep508::{InvalidNameError, PackageName};

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::str::FromStr;

use fs_err::File;
use thiserror::Error;
use tracing::{debug, warn};

use uv_install_wheel::read_record_file;

pub use receipt::ToolReceipt;
pub use tool::{Tool, ToolEntrypoint};
use uv_cache::Cache;
use uv_fs::{LockedFile, Simplified};
use uv_installer::SitePackages;
use uv_python::{Interpreter, PythonEnvironment};
use uv_state::{StateBucket, StateStore};
use uv_static::EnvVars;

mod receipt;
mod tool;

#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("Failed to update `uv-receipt.toml` at {0}")]
    ReceiptWrite(PathBuf, #[source] Box<toml_edit::ser::Error>),
    #[error("Failed to read `uv-receipt.toml` at {0}")]
    ReceiptRead(PathBuf, #[source] Box<toml::de::Error>),
    #[error(transparent)]
    VirtualEnvError(#[from] uv_virtualenv::Error),
    #[error("Failed to read package entry points {0}")]
    EntrypointRead(#[from] uv_install_wheel::Error),
    #[error("Failed to find a directory to install executables into")]
    NoExecutableDirectory,
    #[error(transparent)]
    ToolName(#[from] InvalidNameError),
    #[error(transparent)]
    EnvironmentError(#[from] uv_python::Error),
    #[error("Failed to find a receipt for tool `{0}` at {1}")]
    MissingToolReceipt(String, PathBuf),
    #[error("Failed to read tool environment packages at `{0}`: {1}")]
    EnvironmentRead(PathBuf, String),
    #[error("Failed find package `{0}` in tool environment")]
    MissingToolPackage(PackageName),
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
        if let Some(tool_dir) = std::env::var_os(EnvVars::UV_TOOL_DIR) {
            Ok(Self::from_path(std::path::absolute(tool_dir)?))
        } else {
            Ok(Self::from_path(
                StateStore::from_settings(None)?.bucket(StateBucket::Tools),
            ))
        }
    }

    /// Return the expected directory for a tool with the given [`PackageName`].
    pub fn tool_dir(&self, name: &PackageName) -> PathBuf {
        self.root.join(name.to_string())
    }

    /// Return the metadata for all installed tools.
    ///
    /// If a tool is present, but is missing a receipt or the receipt is invalid, the tool will be
    /// included with an error.
    ///
    /// Note it is generally incorrect to use this without [`Self::acquire_lock`].
    #[allow(clippy::type_complexity)]
    pub fn tools(&self) -> Result<Vec<(PackageName, Result<Tool, Error>)>, Error> {
        let mut tools = Vec::new();
        for directory in uv_fs::directories(self.root())? {
            let Some(name) = directory
                .file_name()
                .and_then(|file_name| file_name.to_str())
            else {
                continue;
            };
            let name = PackageName::from_str(name)?;
            let path = directory.join("uv-receipt.toml");
            let contents = match fs_err::read_to_string(&path) {
                Ok(contents) => contents,
                Err(err) if err.kind() == io::ErrorKind::NotFound => {
                    let err = Error::MissingToolReceipt(name.to_string(), path);
                    tools.push((name, Err(err)));
                    continue;
                }
                Err(err) => return Err(err.into()),
            };
            match ToolReceipt::from_string(contents) {
                Ok(tool_receipt) => tools.push((name, Ok(tool_receipt.tool))),
                Err(err) => {
                    let err = Error::ReceiptRead(path, Box::new(err));
                    tools.push((name, Err(err)));
                }
            }
        }
        Ok(tools)
    }

    /// Get the receipt for the given tool.
    ///
    /// If the tool is not installed, returns `Ok(None)`. If the receipt is invalid, returns an
    /// error.
    ///
    /// Note it is generally incorrect to use this without [`Self::acquire_lock`].
    pub fn get_tool_receipt(&self, name: &PackageName) -> Result<Option<Tool>, Error> {
        let path = self.tool_dir(name).join("uv-receipt.toml");
        match ToolReceipt::from_path(&path) {
            Ok(tool_receipt) => Ok(Some(tool_receipt.tool)),
            Err(Error::Io(err)) if err.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(err),
        }
    }

    /// Grab a file lock for the tools directory to prevent concurrent access across processes.
    pub async fn lock(&self) -> Result<LockedFile, Error> {
        Ok(LockedFile::acquire(self.root.join(".lock"), self.root.user_display()).await?)
    }

    /// Add a receipt for a tool.
    ///
    /// Any existing receipt will be replaced.
    ///
    /// Note it is generally incorrect to use this without [`Self::acquire_lock`].
    pub fn add_tool_receipt(&self, name: &PackageName, tool: Tool) -> Result<(), Error> {
        let tool_receipt = ToolReceipt::from(tool);
        let path = self.tool_dir(name).join("uv-receipt.toml");

        debug!(
            "Adding metadata entry for tool `{name}` at {}",
            path.user_display()
        );

        let doc = tool_receipt
            .to_toml()
            .map_err(|err| Error::ReceiptWrite(path.clone(), Box::new(err)))?;

        // Save the modified `uv-receipt.toml`.
        fs_err::write(&path, doc)?;

        Ok(())
    }

    /// Remove the environment for a tool.
    ///
    /// Does not remove the tool's entrypoints.
    ///
    /// Note it is generally incorrect to use this without [`Self::acquire_lock`].
    ///
    /// # Errors
    ///
    /// If no such environment exists for the tool.
    pub fn remove_environment(&self, name: &PackageName) -> Result<(), Error> {
        let environment_path = self.tool_dir(name);

        debug!(
            "Deleting environment for tool `{name}` at {}",
            environment_path.user_display()
        );

        // On Windows, if the current executable is in the directory, guard against self-deletion.
        #[cfg(windows)]
        if let Ok(itself) = std::env::current_exe() {
            let target = std::path::absolute(&environment_path)?;
            if itself.starts_with(&target) {
                debug!("Detected self-delete of executable: {}", itself.display());
                self_replace::self_delete_outside_path(&environment_path)?;
            }
        }

        fs_err::remove_dir_all(environment_path)?;

        Ok(())
    }

    /// Return the [`PythonEnvironment`] for a given tool, if it exists.
    ///
    /// Returns `Ok(None)` if the environment does not exist or is linked to a non-existent
    /// interpreter.
    ///
    /// Note it is generally incorrect to use this without [`Self::acquire_lock`].
    pub fn get_environment(
        &self,
        name: &PackageName,
        cache: &Cache,
    ) -> Result<Option<PythonEnvironment>, Error> {
        let environment_path = self.tool_dir(name);

        match PythonEnvironment::from_root(&environment_path, cache) {
            Ok(venv) => {
                debug!(
                    "Found existing environment for tool `{name}`: {}",
                    environment_path.user_display()
                );
                Ok(Some(venv))
            }
            Err(uv_python::Error::MissingEnvironment(_)) => Ok(None),
            Err(uv_python::Error::Query(uv_python::InterpreterError::NotFound(
                interpreter_path,
            ))) => {
                if interpreter_path.is_symlink() {
                    let target_path = fs_err::read_link(&interpreter_path)?;
                    warn!(
                        "Ignoring existing virtual environment linked to non-existent Python interpreter: {} -> {}",
                        interpreter_path.user_display(),
                        target_path.user_display()
                    );
                } else {
                    warn!(
                        "Ignoring existing virtual environment with missing Python interpreter: {}",
                        interpreter_path.user_display()
                    );
                }

                Ok(None)
            }
            Err(err) => Err(err.into()),
        }
    }

    /// Create the [`PythonEnvironment`] for a given tool, removing any existing environments.
    ///
    /// Note it is generally incorrect to use this without [`Self::acquire_lock`].
    pub fn create_environment(
        &self,
        name: &PackageName,
        interpreter: Interpreter,
    ) -> Result<PythonEnvironment, Error> {
        let environment_path = self.tool_dir(name);

        // Remove any existing environment.
        match fs_err::remove_dir_all(&environment_path) {
            Ok(()) => {
                debug!(
                    "Removed existing environment for tool `{name}`: {}",
                    environment_path.user_display()
                );
            }
            Err(err) if err.kind() == io::ErrorKind::NotFound => (),
            Err(err) => return Err(err.into()),
        }

        debug!(
            "Creating environment for tool `{name}`: {}",
            environment_path.user_display()
        );

        // Create a virtual environment.
        let venv = uv_virtualenv::create_venv(
            &environment_path,
            interpreter,
            uv_virtualenv::Prompt::None,
            false,
            false,
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

    /// Return the [`Version`] of an installed tool.
    pub fn version(&self, name: &PackageName, cache: &Cache) -> Result<Version, Error> {
        let environment_path = self.tool_dir(name);
        let environment = PythonEnvironment::from_root(&environment_path, cache)?;
        let site_packages = SitePackages::from_environment(&environment)
            .map_err(|err| Error::EnvironmentRead(environment_path.clone(), err.to_string()))?;
        let packages = site_packages.get_packages(name);
        let package = packages
            .first()
            .ok_or_else(|| Error::MissingToolPackage(name.clone()))?;
        Ok(package.version().clone())
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

/// Find the tool executable directory.
pub fn tool_executable_dir() -> Result<PathBuf, Error> {
    user_executable_directory(Some(EnvVars::UV_TOOL_BIN_DIR)).ok_or(Error::NoExecutableDirectory)
}

/// Find the `.dist-info` directory for a package in an environment.
fn find_dist_info<'a>(
    site_packages: &'a SitePackages,
    package_name: &PackageName,
    package_version: &Version,
) -> Result<&'a Path, Error> {
    site_packages
        .get_packages(package_name)
        .iter()
        .find(|package| package.version() == package_version)
        .map(|dist| dist.install_path())
        .ok_or_else(|| Error::MissingToolPackage(package_name.clone()))
}

/// Find the paths to the entry points provided by a package in an environment.
///
/// Entry points can either be true Python entrypoints (defined in `entrypoints.txt`) or scripts in
/// the `.data` directory.
///
/// Returns a list of `(name, path)` tuples.
pub fn entrypoint_paths(
    site_packages: &SitePackages,
    package_name: &PackageName,
    package_version: &Version,
) -> Result<Vec<(String, PathBuf)>, Error> {
    // Find the `.dist-info` directory in the installed environment.
    let dist_info_path = find_dist_info(site_packages, package_name, package_version)?;
    debug!(
        "Looking at `.dist-info` at: {}",
        dist_info_path.user_display()
    );

    // Read the RECORD file.
    let record = read_record_file(&mut File::open(dist_info_path.join("RECORD"))?)?;

    // The RECORD file uses relative paths, so we're looking for the relative path to be a prefix.
    let layout = site_packages.interpreter().layout();
    let script_relative = pathdiff::diff_paths(&layout.scheme.scripts, &layout.scheme.purelib)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::Other,
                format!(
                    "Could not find relative path for: {}",
                    layout.scheme.scripts.simplified_display()
                ),
            )
        })?;

    // Identify any installed binaries (both entrypoints and scripts from the `.data` directory).
    let mut entrypoints = vec![];
    for entry in record {
        let relative_path = PathBuf::from(&entry.path);
        let Ok(path_in_scripts) = relative_path.strip_prefix(&script_relative) else {
            continue;
        };

        let absolute_path = layout.scheme.scripts.join(path_in_scripts);
        let script_name = relative_path
            .file_name()
            .and_then(|filename| filename.to_str())
            .map(ToString::to_string)
            .unwrap_or(entry.path);
        entrypoints.push((script_name, absolute_path));
    }

    Ok(entrypoints)
}
