use std::env::consts::EXE_SUFFIX;
use std::ops::Deref;
use std::path::Path;
use std::path::PathBuf;

use platform_host::{Os, Platform};

/// A Python-aware wrapper around [`Platform`].
#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct PythonPlatform(pub(crate) Platform);

impl PythonPlatform {
    /// Returns the path to the `python` executable inside a virtual environment.
    pub(crate) fn venv_python(&self, venv_root: impl AsRef<Path>) -> PathBuf {
        self.venv_bin_dir(venv_root)
            .join(format!("python{EXE_SUFFIX}"))
    }

    /// Returns the directory in which the binaries are stored inside a virtual environment.
    pub(crate) fn venv_bin_dir(&self, venv_root: impl AsRef<Path>) -> PathBuf {
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
    pub(crate) fn venv_site_packages(
        &self,
        venv_root: impl AsRef<Path>,
        version: (u8, u8),
    ) -> PathBuf {
        let venv = venv_root.as_ref();
        if matches!(self.0.os(), Os::Windows) {
            venv.join("Lib").join("site-packages")
        } else {
            venv.join("lib")
                .join(format!("python{}.{}", version.0, version.1))
                .join("site-packages")
        }
    }
}

impl From<Platform> for PythonPlatform {
    fn from(platform: Platform) -> Self {
        Self(platform)
    }
}

impl Deref for PythonPlatform {
    type Target = Platform;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
