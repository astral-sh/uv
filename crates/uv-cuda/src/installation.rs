use std::path::{Path, PathBuf};

use crate::managed::ManagedCudaInstallation;
use crate::version::CudaVersion;

#[derive(Debug, Clone)]
pub struct CudaInstallation {
    path: PathBuf,
    version: CudaVersion,
}

impl CudaInstallation {
    /// create a new CUDA installation
    pub fn new(path: PathBuf, version: CudaVersion) -> Self {
        Self { path, version }
    }

    /// create a CUDA installation from a managed installation
    pub fn from_managed(managed: ManagedCudaInstallation) -> Self {
        Self {
            path: managed.path().to_path_buf(),
            version: managed.version().clone(),
        }
    }

    /// return the CUDA version.
    pub fn version(&self) -> &CudaVersion {
        &self.version
    }

    /// return the path to the CUDA installation.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// return the path to the CUDA bin directory.
    pub fn bin_dir(&self) -> PathBuf {
        self.path.join("bin")
    }

    /// return the path to the CUDA lib directory.
    pub fn lib_dir(&self) -> PathBuf {
        if cfg!(target_pointer_width = "64") {
            self.path.join("lib64")
        } else {
            self.path.join("lib")
        }
    }

    /// return the path to the CUDA include directory.
    pub fn include_dir(&self) -> PathBuf {
        self.path.join("include")
    }

    /// return the path to nvcc compiler.
    pub fn nvcc(&self) -> PathBuf {
        self.bin_dir().join("nvcc")
    }

    /// set up environment variables for this CUDA installation
    pub fn setup_environment(&self) -> Vec<(String, String)> {
        vec![
            ("CUDA_HOME".to_string(), self.path.display().to_string()),
            ("CUDA_ROOT".to_string(), self.path.display().to_string()),
            ("CUDA_PATH".to_string(), self.path.display().to_string()),
        ]
    }

    /// check if this CUDA installation is valid
    /// TODO(alpin): validate if this holds true for all CUDA versions
    /// especially <= 11.8
    pub fn is_valid(&self) -> bool {
        self.nvcc().exists() && self.lib_dir().exists() && self.include_dir().exists()
    }
}

impl std::fmt::Display for CudaInstallation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "cuda-{} ({})", self.version, self.path.display())
    }
}
