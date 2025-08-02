use std::io;
use std::path::{Path, PathBuf};

use fs_err as fs;
use thiserror::Error;

use uv_fs::{LockedFile, Simplified};
use uv_state::{StateBucket, StateStore};

use crate::version::CudaVersion;

/// Managed CUDA installations in a designated directory.
#[derive(Debug, Clone)]
pub struct ManagedCudaInstallations {
    root: PathBuf,
}

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] io::Error),

    #[error("Invalid CUDA installation directory")]
    InvalidInstallation,

    #[error("No executable directory found")]
    NoExecutableDirectory,
}

impl ManagedCudaInstallations {
    /// a directory for CUDA installations at `root`.
    fn from_path(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// file lock to prevent concurrent access across processes.
    pub async fn lock(&self) -> Result<LockedFile, Error> {
        Ok(LockedFile::acquire(self.root.join(".lock"), self.root.user_display()).await?)
    }

    /// Prefer, in order:
    ///
    /// 1. The specific CUDA directory passed via the `install_dir` argument.
    /// 2. The specific CUDA directory specified with the `UV_CUDA_INSTALL_DIR` environment variable.
    /// 3. A directory in the system-appropriate user-level data directory, e.g., `~/.local/uv/cuda`.
    /// 4. A directory in the local data directory, e.g., `./.uv/cuda`.
    pub fn from_settings(install_dir: Option<PathBuf>) -> Result<Self, Error> {
        if let Some(install_dir) = install_dir {
            Ok(Self::from_path(install_dir))
        } else if let Some(install_dir) =
            std::env::var_os("UV_CUDA_INSTALL_DIR").filter(|s| !s.is_empty())
        {
            Ok(Self::from_path(install_dir))
        } else {
            Ok(Self::from_path(
                StateStore::from_settings(None)?.bucket(StateBucket::Cuda),
            ))
        }
    }

    /// temporary CUDA installation directory
    pub fn temp() -> Result<Self, Error> {
        Ok(Self::from_path(
            StateStore::temp()?.bucket(StateBucket::Cuda),
        ))
    }

    /// return the location of the scratch directory for managed CUDA installations
    pub fn scratch(&self) -> PathBuf {
        self.root.join(".temp")
    }

    /// initialize the CUDA installation directory
    pub fn init(self) -> Result<Self, Error> {
        let root = &self.root;

        // create the directory, if it doesn't exist
        fs::create_dir_all(root)?;

        // create the scratch directory, if it doesn't exist
        let scratch = self.scratch();
        fs::create_dir_all(&scratch)?;

        // add a .gitignore
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(root.join(".gitignore"))
        {
            Ok(mut file) => std::io::Write::write_all(&mut file, b"*")?,
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => (),
            Err(err) => return Err(err.into()),
        }

        Ok(self)
    }

    /// iterate over each CUDA installation in this directory
    /// CUDA versions are sorted in descending order (newest first)
    pub fn find_all(&self) -> Result<impl Iterator<Item = ManagedCudaInstallation>, Error> {
        let mut installations = Vec::new();

        let entries = match fs::read_dir(&self.root) {
            Ok(entries) => entries,
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                return Ok(installations.into_iter());
            }
            Err(err) => return Err(err.into()),
        };

        let scratch = self.scratch();

        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() && path != scratch {
                // Ignore any `.` prefixed directories
                if let Some(name) = path.file_name().and_then(std::ffi::OsStr::to_str) {
                    if !name.starts_with('.') {
                        if let Ok(installation) = ManagedCudaInstallation::from_path(path) {
                            installations.push(installation);
                        }
                    }
                }
            }
        }

        Ok(installations.into_iter())
    }

    /// find a CUDA installation for the given version
    pub fn find_version(
        &self,
        version: &CudaVersion,
    ) -> Result<Option<ManagedCudaInstallation>, Error> {
        for installation in self.find_all()? {
            if installation.version().matches(version) {
                return Ok(Some(installation));
            }
        }
        Ok(None)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
}

/// uv-managed CUDA installation on the current system
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct ManagedCudaInstallation {
    /// The path to the top-level directory of the installed CUDA.
    path: PathBuf,
    /// The CUDA version.
    version: CudaVersion,
    /// The URL with the CUDA archive.
    url: Option<&'static str>,
    /// The SHA256 of the CUDA archive at the URL.
    sha256: Option<&'static str>,
}

impl ManagedCudaInstallation {
    /// create a [`ManagedCudaInstallation`] from a directory
    pub fn from_path(path: PathBuf) -> Result<Self, Error> {
        // extract version from directory name
        let version_str = path
            .file_name()
            .and_then(std::ffi::OsStr::to_str)
            .ok_or(Error::InvalidInstallation)?;

        // parse version - expect format like "cuda-12.8" or just "12.8"
        let version_str = version_str.strip_prefix("cuda-").unwrap_or(version_str);
        let version = version_str
            .parse::<CudaVersion>()
            .map_err(|_| Error::InvalidInstallation)?;

        Ok(Self {
            path,
            version,
            url: None,
            sha256: None,
        })
    }

    /// create a new [`ManagedCudaInstallation`] with the given metadata
    pub fn new(
        path: PathBuf,
        version: CudaVersion,
        url: Option<&'static str>,
        sha256: Option<&'static str>,
    ) -> Self {
        Self {
            path,
            version,
            url,
            sha256,
        }
    }

    pub fn version(&self) -> &CudaVersion {
        &self.version
    }

    /// return the path to the CUDA installation
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// return the URL of the CUDA archive, if available
    pub fn url(&self) -> Option<&'static str> {
        self.url
    }

    /// return the SHA256 hash of the CUDA archive, if available
    pub fn sha256(&self) -> Option<&'static str> {
        self.sha256
    }

    /// return the path to the CUDA bin directory
    pub fn bin_dir(&self) -> PathBuf {
        self.path.join("bin")
    }

    /// return the path to the CUDA lib directory
    pub fn lib_dir(&self) -> PathBuf {
        if cfg!(target_pointer_width = "64") {
            self.path.join("lib64")
        } else {
            self.path.join("lib")
        }
    }

    /// return the path to the CUDA include directory
    pub fn include_dir(&self) -> PathBuf {
        self.path.join("include")
    }

    /// return the path to nvcc compiler
    pub fn nvcc(&self) -> PathBuf {
        self.bin_dir().join("nvcc")
    }

    /// Set up environment variables for this CUDA installation.
    /// Reference: https://gist.github.com/jvmncs/f0f32dcbb38e7bccd5fb076f0ae840ee
    pub fn setup_environment(&self) -> Vec<(String, String)> {
        let cuda_path = self.path.display().to_string();
        let bin_path = self.bin_dir().display().to_string();
        let lib_path = self.lib_dir().display().to_string();
        let _include_path = self.include_dir().display().to_string();

        vec![
            ("CUDA_HOME".to_string(), cuda_path.clone()),
            ("CUDA_ROOT".to_string(), cuda_path.clone()),
            ("CUDA_PATH".to_string(), cuda_path.clone()),
            ("PATH".to_string(), format!("{}:$PATH", bin_path)),
            (
                "LD_LIBRARY_PATH".to_string(),
                format!("{}:$LD_LIBRARY_PATH", lib_path),
            ),
            (
                "PKG_CONFIG_PATH".to_string(),
                format!("{}/pkg-config:$PKG_CONFIG_PATH", lib_path),
            ),
            (
                "XLA_FLAGS".to_string(),
                format!("--xla_gpu_cuda_data_dir={}", cuda_path),
            ),
        ]
    }

    pub fn env_file_path(&self) -> std::path::PathBuf {
        self.path.join("env.sh")
    }

    pub fn generate_env_file(&self) -> String {
        let cuda_path = self.path.display().to_string();
        let bin_path = self.bin_dir().display().to_string();
        let lib_path = self.lib_dir().display().to_string();

        format!(
            "# CUDA {} environment file generated by uv\n\
# Generated on: {}\n\
# CUDA installation path: {}\n\
\n\
export CUDA_HOME=\"{}\"\n\
export CUDA_ROOT=\"{}\"\n\
export CUDA_PATH=\"{}\"\n\
export PATH=\"{}:$PATH\"\n\
export LD_LIBRARY_PATH=\"{}:$LD_LIBRARY_PATH\"\n\
export PKG_CONFIG_PATH=\"{}/pkg-config:$PKG_CONFIG_PATH\"\n\
export XLA_FLAGS=\"--xla_gpu_cuda_data_dir={}\"\n\
\n\
# NVCC compiler available at: {}\n\
# CUDA libraries available at: {}\n\
# CUDA headers available at: {}\n\
",
            self.version,
            chrono::Utc::now().to_rfc3339(),
            cuda_path,
            cuda_path,
            cuda_path,
            cuda_path,
            bin_path,
            lib_path,
            lib_path,
            cuda_path,
            self.nvcc().display(),
            self.lib_dir().display(),
            self.include_dir().display()
        )
    }

    /// Check if this CUDA installation is valid.
    pub fn is_valid(&self) -> bool {
        self.nvcc().exists() && self.lib_dir().exists() && self.include_dir().exists()
    }
}

impl std::fmt::Display for ManagedCudaInstallation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "cuda-{}", self.version)
    }
}
