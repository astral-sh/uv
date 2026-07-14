use std::ffi::OsStr;
use std::fmt::{self, Display, Formatter};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::str::FromStr;

use fs_err as fs;
use fs_err::File;
use owo_colors::OwoColorize;
use thiserror::Error;
use tracing::{debug, warn};

use uv_cache::Cache;
use uv_dirs::user_executable_directory;
use uv_fs::{LockedFile, LockedFileError, LockedFileMode, Simplified};
use uv_install_wheel::read_record;
use uv_installer::SitePackages;
use uv_normalize::PackageName;
use uv_pep440::Version;
use uv_python::{BrokenLink, Interpreter, PythonEnvironment};
use uv_state::{StateBucket, StateStore};
use uv_static::EnvVars;

pub(crate) use receipt::ToolReceipt;
pub use tool::{Tool, ToolEntrypoint};

mod receipt;
mod tool;

/// The name of an installed tool.
///
/// Unlike a [`PackageName`], a tool name includes any user-provided suffix and is not normalized.
/// It is used as the name of the tool's environment directory and as the identifier accepted by
/// commands such as `uv tool upgrade` and `uv tool uninstall`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ToolName(String);

impl ToolName {
    /// Create a tool name for a package and optional suffix.
    pub fn from_package_name(
        package: &PackageName,
        suffix: Option<&str>,
    ) -> Result<Self, InvalidToolNameError> {
        if suffix.is_some_and(str::is_empty) {
            return Err(InvalidToolNameError::empty_suffix());
        }

        let mut name = package.to_string();
        if let Some(suffix) = suffix {
            name.push_str(suffix);
        }
        Self::from_string(name)
    }

    /// Return the tool name as a string.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn from_string(name: String) -> Result<Self, InvalidToolNameError> {
        // Tool names are used as directory and executable names. Validate against the portable
        // subset of filenames so a receipt created on one platform can be managed on another.
        if name.is_empty()
            || matches!(name.as_str(), "." | "..")
            || name.ends_with(['.', ' '])
            || is_windows_reserved_device_name(&name)
            || name.bytes().any(|character| {
                character.is_ascii_control()
                    || matches!(
                        character,
                        b'/' | b'\\' | b'<' | b'>' | b':' | b'"' | b'|' | b'?' | b'*'
                    )
            })
        {
            return Err(InvalidToolNameError::invalid(&name));
        }

        Ok(Self(name))
    }
}

fn is_windows_reserved_device_name(name: &str) -> bool {
    let basename = name
        .split_once('.')
        .map_or(name, |(basename, _)| basename)
        .to_ascii_uppercase();

    if matches!(basename.as_str(), "CON" | "PRN" | "AUX" | "NUL") {
        return true;
    }

    basename
        .strip_prefix("COM")
        .or_else(|| basename.strip_prefix("LPT"))
        .is_some_and(|port| {
            matches!(
                port,
                "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
            )
        })
}

impl From<&PackageName> for ToolName {
    fn from(package: &PackageName) -> Self {
        Self(package.to_string())
    }
}

impl From<PackageName> for ToolName {
    fn from(package: PackageName) -> Self {
        Self(package.to_string())
    }
}

impl FromStr for ToolName {
    type Err = InvalidToolNameError;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        Self::from_string(name.to_string())
    }
}

impl AsRef<str> for ToolName {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Display for ToolName {
    fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// An invalid [`ToolName`].
#[derive(Debug, Clone, Error)]
#[error("{message}")]
pub struct InvalidToolNameError {
    message: String,
}

impl InvalidToolNameError {
    fn invalid(name: &str) -> Self {
        Self {
            message: format!(
                "Invalid tool name `{name}`; tool names must be valid cross-platform filenames"
            ),
        }
    }

    fn empty_suffix() -> Self {
        Self {
            message: "Tool suffix cannot be empty".to_string(),
        }
    }
}

/// A wrapper around [`PythonEnvironment`] for tools that provides additional functionality.
#[derive(Debug, Clone)]
pub struct ToolEnvironment {
    environment: PythonEnvironment,
    name: PackageName,
}

impl ToolEnvironment {
    fn new(environment: PythonEnvironment, name: PackageName) -> Self {
        Self { environment, name }
    }

    /// Return the [`Version`] of the tool package in this environment.
    pub fn version(&self) -> Result<Version, Error> {
        let site_packages = SitePackages::from_environment(&self.environment).map_err(|err| {
            Error::EnvironmentRead(self.environment.root().to_path_buf(), err.to_string())
        })?;
        let packages = site_packages.get_packages(&self.name);
        let package = packages
            .first()
            .ok_or_else(|| Error::MissingToolPackage(self.name.clone()))?;
        Ok(package.version().clone())
    }

    /// Get the underlying [`PythonEnvironment`].
    pub fn into_environment(self) -> PythonEnvironment {
        self.environment
    }

    /// Get a reference to the underlying [`PythonEnvironment`].
    pub fn environment(&self) -> &PythonEnvironment {
        &self.environment
    }
}

#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    LockedFile(#[from] LockedFileError),
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
    ToolName(#[from] InvalidToolNameError),
    #[error(transparent)]
    EnvironmentError(#[from] uv_python::Error),
    #[error("Failed to find a receipt for tool `{0}` at {1}")]
    MissingToolReceipt(String, PathBuf),
    #[error("Failed to read tool environment packages at `{0}`: {1}")]
    EnvironmentRead(PathBuf, String),
    #[error("Failed find package `{0}` in tool environment")]
    MissingToolPackage(PackageName),
    #[error("Tool `{0}` environment not found at `{1}`")]
    ToolEnvironmentNotFound(PackageName, PathBuf),
}

impl Error {
    pub fn as_io_error(&self) -> Option<&io::Error> {
        match self {
            Self::Io(err) => Some(err),
            Self::LockedFile(err) => err.as_io_error(),
            Self::VirtualEnvError(uv_virtualenv::Error::Io(err)) => Some(err),
            Self::ReceiptWrite(_, _)
            | Self::ReceiptRead(_, _)
            | Self::VirtualEnvError(_)
            | Self::EntrypointRead(_)
            | Self::NoExecutableDirectory
            | Self::ToolName(_)
            | Self::EnvironmentError(_)
            | Self::MissingToolReceipt(_, _)
            | Self::EnvironmentRead(_, _)
            | Self::MissingToolPackage(_)
            | Self::ToolEnvironmentNotFound(_, _) => None,
        }
    }
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
        if let Some(tool_dir) = std::env::var_os(EnvVars::UV_TOOL_DIR).filter(|s| !s.is_empty()) {
            Ok(Self::from_path(std::path::absolute(tool_dir)?))
        } else {
            Ok(Self::from_path(
                StateStore::from_settings(None)?.bucket(StateBucket::Tools),
            ))
        }
    }

    /// Return the expected directory for a tool with the given [`ToolName`].
    pub fn tool_dir(&self, name: &ToolName) -> PathBuf {
        self.root.join(name.as_str())
    }

    /// Return the metadata for all installed tools.
    ///
    /// If a tool is present, but is missing a receipt or the receipt is invalid, the tool will be
    /// included with an error.
    ///
    /// Note it is generally incorrect to use this without [`Self::acquire_lock`].
    #[expect(clippy::type_complexity)]
    pub fn tools(&self) -> Result<Vec<(ToolName, Result<Tool, Error>)>, Error> {
        let mut tools = Vec::new();
        for directory in uv_fs::directories(self.root())? {
            let Some(name) = directory
                .file_name()
                .and_then(|file_name| file_name.to_str())
            else {
                continue;
            };
            let name = ToolName::from_str(name)?;
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

    /// Return an installed tool with a name that differs only by ASCII case.
    pub fn find_case_insensitive_name(&self, name: &ToolName) -> Result<Option<ToolName>, Error> {
        for directory in uv_fs::directories(self.root())? {
            let Some(existing_name) = directory
                .file_name()
                .and_then(OsStr::to_str)
                .and_then(|name| ToolName::from_str(name).ok())
            else {
                continue;
            };
            if existing_name != *name && existing_name.as_str().eq_ignore_ascii_case(name.as_str())
            {
                return Ok(Some(existing_name));
            }
        }
        Ok(None)
    }

    /// Get the receipt for the given tool.
    ///
    /// If the tool is not installed, returns `Ok(None)`. If the receipt is invalid, returns an
    /// error.
    ///
    /// Note it is generally incorrect to use this without [`Self::acquire_lock`].
    pub fn get_tool_receipt(&self, name: &ToolName) -> Result<Option<Tool>, Error> {
        let path = self.tool_dir(name).join("uv-receipt.toml");
        match ToolReceipt::from_path(&path) {
            Ok(tool_receipt) => Ok(Some(tool_receipt.tool)),
            Err(Error::Io(err)) if err.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(err),
        }
    }

    /// Grab a file lock for the tools directory to prevent concurrent access across processes.
    pub async fn lock(&self) -> Result<LockedFile, Error> {
        Ok(LockedFile::acquire(
            self.root.join(".lock"),
            LockedFileMode::Exclusive,
            self.root.user_display(),
        )
        .await?)
    }

    /// Add a receipt for a tool.
    ///
    /// Any existing receipt will be replaced.
    ///
    /// Note it is generally incorrect to use this without [`Self::acquire_lock`].
    pub fn add_tool_receipt(&self, name: &ToolName, tool: Tool) -> Result<(), Error> {
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
    pub fn remove_environment(&self, name: &ToolName) -> Result<(), Error> {
        let environment_path = self.tool_dir(name);

        debug!(
            "Deleting environment for tool `{name}` at {}",
            environment_path.user_display()
        );

        uv_fs::remove_virtualenv(environment_path.as_path()).map_err(uv_virtualenv::Error::from)?;

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
        name: &ToolName,
        package: &PackageName,
        cache: &Cache,
    ) -> Result<Option<ToolEnvironment>, Error> {
        let environment_path = self.tool_dir(name);

        match PythonEnvironment::from_root(&environment_path, cache) {
            Ok(venv) => {
                debug!(
                    "Found existing environment for tool `{name}`: {}",
                    environment_path.user_display()
                );
                Ok(Some(ToolEnvironment::new(venv, package.clone())))
            }
            Err(uv_python::Error::MissingEnvironment(_)) => Ok(None),
            Err(uv_python::Error::Query(uv_python::InterpreterError::NotFound(
                interpreter_path,
            ))) => {
                warn!(
                    "Ignoring existing virtual environment with missing Python interpreter: {}",
                    interpreter_path.user_display()
                );

                Ok(None)
            }
            Err(uv_python::Error::Query(uv_python::InterpreterError::BrokenLink(BrokenLink {
                path,
                unix,
                venv: _,
            }))) => {
                if unix {
                    let target_path = fs_err::read_link(&path)?;
                    warn!(
                        "Ignoring existing virtual environment linked to non-existent Python interpreter: {} -> {}",
                        path.user_display().cyan(),
                        target_path.user_display().cyan(),
                    );
                } else {
                    warn!(
                        "Ignoring existing virtual environment linked to non-existent Python interpreter: {}",
                        path.user_display().cyan(),
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
        name: &ToolName,
        interpreter: Interpreter,
    ) -> Result<PythonEnvironment, Error> {
        let environment_path = self.tool_dir(name);

        // Remove any existing environment.
        match uv_fs::remove_virtualenv(&environment_path) {
            Ok(()) => {
                debug!(
                    "Removed existing environment for tool `{name}`: {}",
                    environment_path.user_display()
                );
            }
            Err(err) if err.kind() == io::ErrorKind::NotFound => (),
            Err(err) => return Err(uv_virtualenv::Error::from(err).into()),
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
            uv_virtualenv::OnExisting::Remove(uv_virtualenv::RemovalReason::ManagedEnvironment),
            false,
            false,
            false,
        )?;

        Ok(venv)
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
    let record = read_record(File::open(dist_info_path.join("RECORD"))?)?;

    // The RECORD file uses relative paths, so we're looking for the relative path to be a prefix.
    let layout = site_packages.interpreter().layout();
    let script_relative = pathdiff::diff_paths(&layout.scheme.scripts, &layout.scheme.purelib)
        .ok_or_else(|| {
            io::Error::other(format!(
                "Could not find relative path for: {}",
                layout.scheme.scripts.simplified_display()
            ))
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

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::ToolName;

    #[test]
    fn windows_reserved_device_names() {
        for name in [
            "con",
            "CON.txt",
            "prn.log",
            "aux",
            "nul.json",
            "com1",
            "COM9.exe",
            "com¹.log",
            "lpt1",
            "LPT9.txt",
            "lpt³.log",
        ] {
            assert!(
                ToolName::from_str(name).is_err(),
                "`{name}` should be rejected"
            );
        }

        for name in ["console", "com0", "com10", "lpt0", "lpt10", "com1port"] {
            assert!(ToolName::from_str(name).is_ok(), "`{name}` should be valid");
        }
    }
}
