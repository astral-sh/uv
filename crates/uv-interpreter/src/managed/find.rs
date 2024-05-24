use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::path::PathBuf;
use std::str::FromStr;

use once_cell::sync::Lazy;
use tracing::debug;

use uv_fs::Simplified;

use crate::managed::downloads::Error;
use crate::platform::{Arch, Libc, Os};
use crate::python_version::PythonVersion;

/// The directory where Python toolchains we install are stored.
pub static TOOLCHAIN_DIRECTORY: Lazy<Option<PathBuf>> =
    Lazy::new(|| std::env::var_os("UV_BOOTSTRAP_DIR").map(PathBuf::from));

pub fn toolchains_for_current_platform() -> Result<impl Iterator<Item = Toolchain>, Error> {
    let platform_key = platform_key_from_env()?;
    let iter = toolchain_directories()?
        .into_iter()
        // Sort "newer" versions of Python first
        .rev()
        .filter_map(move |path| {
            if path
                .file_name()
                .map(OsStr::to_string_lossy)
                .is_some_and(|filename| filename.ends_with(&platform_key))
            {
                Toolchain::new(path.clone())
                    .inspect_err(|err| {
                        debug!(
                            "Ignoring invalid toolchain directory {}: {err}",
                            path.user_display()
                        );
                    })
                    .ok()
            } else {
                None
            }
        });

    Ok(iter)
}

/// An installed Python toolchain.
#[derive(Debug, Clone)]
pub struct Toolchain {
    /// The path to the top-level directory of the installed toolchain.
    path: PathBuf,
    python_version: PythonVersion,
}

impl Toolchain {
    pub fn new(path: PathBuf) -> Result<Self, Error> {
        let python_version = PythonVersion::from_str(
            path.file_name()
                .ok_or(Error::NameError("No directory name".to_string()))?
                .to_str()
                .ok_or(Error::NameError("Name not a valid string".to_string()))?
                .split('-')
                .nth(1)
                .ok_or(Error::NameError(
                    "Not enough `-` separarated values".to_string(),
                ))?,
        )
        .map_err(|err| Error::NameError(format!("Name has invalid Python version: {err}")))?;

        Ok(Self {
            path,
            python_version,
        })
    }
    pub fn executable(&self) -> PathBuf {
        if cfg!(windows) {
            self.path.join("install").join("python.exe")
        } else if cfg!(unix) {
            self.path.join("install").join("bin").join("python3")
        } else {
            unimplemented!("Only Windows and Unix systems are supported.")
        }
    }

    pub fn python_version(&self) -> &PythonVersion {
        &self.python_version
    }
}

/// Return the directories in the toolchain directory.
///
/// Toolchain directories are sorted descending by name, such that we get deterministic
/// ordering across platforms. This also results in newer Python versions coming first,
/// but should not be relied on — instead the toolchains should be sorted later by
/// the parsed Python version.
fn toolchain_directories() -> Result<BTreeSet<PathBuf>, Error> {
    let Some(toolchain_dir) = TOOLCHAIN_DIRECTORY.as_ref() else {
        return Ok(BTreeSet::default());
    };
    match fs_err::read_dir(toolchain_dir.clone()) {
        Ok(toolchain_dirs) => {
            // Collect sorted directory paths; `read_dir` is not stable across platforms
            let directories: BTreeSet<_> = toolchain_dirs
                .filter_map(|read_dir| match read_dir {
                    Ok(entry) => match entry.file_type() {
                        Ok(file_type) => file_type.is_dir().then_some(Ok(entry.path())),
                        Err(err) => Some(Err(err)),
                    },
                    Err(err) => Some(Err(err)),
                })
                .collect::<Result<_, std::io::Error>>()
                .map_err(|err| Error::ReadError {
                    dir: toolchain_dir.clone(),
                    err,
                })?;
            Ok(directories)
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(BTreeSet::default()),
        Err(err) => Err(Error::ReadError {
            dir: toolchain_dir.clone(),
            err,
        }),
    }
}

/// Return the toolchains that satisfy the given Python version on this platform.
///
/// ## Errors
///
/// - The platform metadata cannot be read
/// - A directory in the toolchain directory cannot be read
pub fn toolchains_for_version(version: &PythonVersion) -> Result<Vec<Toolchain>, Error> {
    let platform_key = platform_key_from_env()?;

    // TODO(zanieb): Consider returning an iterator instead of a `Vec`
    //               Note we need to collect paths regardless for sorting by version.

    let toolchain_dirs = toolchain_directories()?;

    Ok(toolchain_dirs
        .into_iter()
        // Sort "newer" versions of Python first
        .rev()
        .filter_map(|path| {
            if path
                .file_name()
                .map(OsStr::to_string_lossy)
                .is_some_and(|filename| {
                    filename.starts_with(&format!("cpython-{version}"))
                        && filename.ends_with(&platform_key)
                })
            {
                Toolchain::new(path.clone())
                    .inspect_err(|err| {
                        debug!(
                            "Ignoring invalid toolchain directory {}: {err}",
                            path.user_display()
                        );
                    })
                    .ok()
            } else {
                None
            }
        })
        .collect::<Vec<_>>())
}

/// Generate a platform portion of a key from the environment.
fn platform_key_from_env() -> Result<String, Error> {
    let os = Os::from_env()?;
    let arch = Arch::from_env()?;
    let libc = Libc::from_env();
    Ok(format!("{os}-{arch}-{libc}").to_lowercase())
}
