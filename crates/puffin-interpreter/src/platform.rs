use std::env;
use std::fmt;
use std::path::Path;
use std::path::PathBuf;

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct Platform {
    os: Option<Os>,
}

impl Platform {
    /// Infer the target based on the current version used for compilation.
    pub(crate) fn from_host() -> Self {
        Self {
            os: if cfg!(windows) {
                Some(Os::Windows)
            } else if cfg!(unix) {
                Some(Os::Linux)
            } else if cfg!(macos) {
                Some(Os::Macos)
            } else {
                None
            },
        }
    }

    /// Returns `true` if the current platform is Linux.
    #[allow(unused)]
    #[inline]
    pub(crate) fn is_linux(&self) -> bool {
        self.os == Some(Os::Linux)
    }

    /// Returns `true` if the current platform is macOS.
    #[allow(unused)]
    #[inline]
    pub(crate) fn is_macos(&self) -> bool {
        self.os == Some(Os::Macos)
    }

    /// Returns `true` if the current platform is Windows.
    #[allow(unused)]
    #[inline]
    pub(crate) fn is_windows(&self) -> bool {
        self.os == Some(Os::Windows)
    }

    /// Returns the path to the `python` executable inside a virtual environment.
    pub(crate) fn get_venv_python(&self, venv_base: impl AsRef<Path>) -> PathBuf {
        self.get_venv_bin_dir(venv_base).join(self.get_python())
    }

    /// Returns the directory in which the binaries are stored inside a virtual environment.
    pub(crate) fn get_venv_bin_dir(&self, venv_base: impl AsRef<Path>) -> PathBuf {
        let venv = venv_base.as_ref();
        if self.is_windows() {
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

    /// Returns the path to the `python` executable.
    ///
    /// For Windows, it's always `python.exe`. For UNIX, it's the `python` in the virtual
    /// environment; or, if there is no virtual environment, `python3`.
    pub(crate) fn get_python(&self) -> PathBuf {
        if self.is_windows() {
            PathBuf::from("python.exe")
        } else if env::var_os("VIRTUAL_ENV").is_some() {
            PathBuf::from("python")
        } else {
            PathBuf::from("python3")
        }
    }
}

/// All supported operating systems.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum Os {
    Linux,
    Windows,
    Macos,
}

impl fmt::Display for Os {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Os::Linux => write!(f, "Linux"),
            Os::Windows => write!(f, "Windows"),
            Os::Macos => write!(f, "macOS"),
        }
    }
}
