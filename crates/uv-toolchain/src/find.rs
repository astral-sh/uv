use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::downloads::{Arch, Error, Libc, Os};
use crate::python_version::PythonVersion;

use once_cell::sync::Lazy;

pub static TOOLCHAIN_DIRECTORY: Lazy<PathBuf> = Lazy::new(|| {
    std::env::var_os("UV_BOOTSTRAP_DIR").map_or(
        Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap())
            .parent()
            .expect("CARGO_MANIFEST_DIR should be nested in workspace")
            .parent()
            .expect("CARGO_MANIFEST_DIR should be doubly nested in workspace")
            .join("bin"),
        PathBuf::from,
    )
});

#[derive(Debug, Clone)]
pub struct Toolchain {
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

// TODO(zanieb): Implement version requests without patch versions
pub fn toolchains_for_version(version: &PythonVersion) -> Result<Vec<Toolchain>, Error> {
    let platform_key = platform_key_from_env()?;

    let toolchain_dirs = match fs_err::read_dir(TOOLCHAIN_DIRECTORY.to_path_buf()) {
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
                    dir: TOOLCHAIN_DIRECTORY.to_path_buf(),
                    err,
                })?;
            directories
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Vec::new());
        }
        Err(err) => {
            return Err(Error::ReadError {
                dir: TOOLCHAIN_DIRECTORY.to_path_buf(),
                err,
            })
        }
    };

    Ok(toolchain_dirs
        .into_iter()
        // Sort "newer" versions of Python first
        .rev()
        .filter_map(|path| {
            if path.file_name().is_some_and(|filename| {
                filename
                    .to_string_lossy()
                    .starts_with(&format!("cpython-{version}"))
                    && filename.to_string_lossy().ends_with(&platform_key)
            }) {
                Some(Toolchain { path })
            } else {
                None
            }
        })
        .collect::<Vec<_>>())
}

fn platform_key_from_env() -> Result<String, Error> {
    let os = Os::from_env()?;
    let arch = Arch::from_env()?;
    let libc = Libc::from_env()?;
    Ok(format!("{os}-{arch}-{libc}").to_lowercase())
}
