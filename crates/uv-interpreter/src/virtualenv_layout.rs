use std::env::consts::EXE_SUFFIX;
use std::path::Path;
use std::path::PathBuf;

use platform_host::{Os, Platform};

/// Construct paths to various locations inside a virtual environment based on the platform.
#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct VirtualenvLayout<'a>(&'a Platform);

impl<'a> VirtualenvLayout<'a> {
    /// Create a new [`VirtualenvLayout`] for the given platform.
    pub(crate) fn from_platform(platform: &'a Platform) -> Self {
        Self(platform)
    }

    /// Returns the path to the `python` executable inside a virtual environment.
    pub(crate) fn python_executable(&self, venv_root: impl AsRef<Path>) -> PathBuf {
        self.scripts(venv_root).join(format!("python{EXE_SUFFIX}"))
    }

    /// Returns the directory in which the binaries are stored inside a virtual environment.
    pub(crate) fn scripts(&self, venv_root: impl AsRef<Path>) -> PathBuf {
        let venv = venv_root.as_ref();
        if matches!(self.0.os(), Os::Windows) {
            let bin_dir = venv.join("Scripts");
            if bin_dir.join("python.exe").exists() {
                return bin_dir;
            }
            // Python installed via msys2 on Windows might produce a POSIX-like venv
            // See https://github.com/PyO3/maturin/issues/1108
            let bin_dir = venv.join("bin");
            if bin_dir.join("python.exe").exists() {
                return bin_dir;
            }
            // for conda environment
            venv.to_path_buf()
        } else {
            venv.join("bin")
        }
    }

    /// Returns the path to the `site-packages` directory inside a virtual environment.
    pub(crate) fn site_packages(&self, venv_root: impl AsRef<Path>, version: (u8, u8)) -> PathBuf {
        let venv = venv_root.as_ref();
        if matches!(self.0.os(), Os::Windows) {
            venv.join("Lib").join("site-packages")
        } else {
            venv.join("lib")
                .join(format!("python{}.{}", version.0, version.1))
                .join("site-packages")
        }
    }

    /// Returns the path to the `data` directory inside a virtual environment.
    #[allow(clippy::unused_self)]
    pub(crate) fn data(&self, venv_root: impl AsRef<Path>) -> PathBuf {
        venv_root.as_ref().to_path_buf()
    }

    /// Returns the path to the `platstdlib` directory inside a virtual environment.
    #[allow(clippy::unused_self)]
    pub(crate) fn platstdlib(&self, venv_root: impl AsRef<Path>, version: (u8, u8)) -> PathBuf {
        let venv = venv_root.as_ref();
        if matches!(self.0.os(), Os::Windows) {
            venv.join("Lib")
        } else {
            venv.join("lib")
                .join(format!("python{}.{}", version.0, version.1))
                .join("site-packages")
        }
    }
}
