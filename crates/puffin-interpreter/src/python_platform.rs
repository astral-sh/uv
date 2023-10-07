use std::path::Path;
use std::path::PathBuf;

use puffin_platform::{Os, Platform};

/// A Python-aware wrapper around [`Platform`].
#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct PythonPlatform<'a>(&'a Platform);

impl PythonPlatform<'_> {
    /// Returns the path to the `python` executable inside a virtual environment.
    pub(crate) fn venv_python(&self, venv_base: impl AsRef<Path>) -> PathBuf {
        let python = if matches!(self.0.os(), Os::Windows) {
            "python.exe"
        } else {
            "python"
        };
        self.venv_bin_dir(venv_base).join(python)
    }

    /// Returns the directory in which the binaries are stored inside a virtual environment.
    pub(crate) fn venv_bin_dir(&self, venv_base: impl AsRef<Path>) -> PathBuf {
        let venv = venv_base.as_ref();
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
}

impl<'a> From<&'a Platform> for PythonPlatform<'a> {
    fn from(platform: &'a Platform) -> Self {
        Self(platform)
    }
}
