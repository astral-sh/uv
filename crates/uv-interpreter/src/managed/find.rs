use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use crate::managed::downloads::Error;
use crate::{
    platform::{Arch, Libc, Os},
    python_version::PythonVersion,
};

use once_cell::sync::Lazy;

/// The directory where Python toolchains we install are stored.
pub static TOOLCHAIN_DIRECTORY: Lazy<Option<PathBuf>> = Lazy::new(|| {
    std::env::var_os("UV_BOOTSTRAP_DIR").map_or(
        std::env::var_os("CARGO_MANIFEST_DIR").map(|manifest_dir| {
            Path::new(&manifest_dir)
                .parent()
                .expect("CARGO_MANIFEST_DIR should be nested in workspace")
                .parent()
                .expect("CARGO_MANIFEST_DIR should be doubly nested in workspace")
                .join("bin")
        }),
        |bootstrap_dir| Some(PathBuf::from(bootstrap_dir)),
    )
});

/// An installed Python toolchain.
#[derive(Debug, Clone)]
pub struct Toolchain {
    /// The path to the top-level directory of the installed toolchain.
    path: PathBuf,
}

impl Toolchain {
    pub fn executable(&self) -> PathBuf {
        if cfg!(windows) {
            self.path.join("install").join("python.exe")
        } else if cfg!(unix) {
            self.path.join("install").join("bin").join("python3")
        } else {
            unimplemented!("Only Windows and Unix systems are supported.")
        }
    }
}

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
                Some(Toolchain { path })
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
    let libc = Libc::from_env()?;
    Ok(format!("{os}-{arch}-{libc}").to_lowercase())
}
