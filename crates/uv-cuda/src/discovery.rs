use std::fmt;
use std::str::FromStr;

use thiserror::Error;

use crate::managed::ManagedCudaInstallations;
use crate::version::CudaVersion;

/// request to find a CUDA installation.
#[derive(Debug, Clone, PartialEq, Eq, Default, Hash)]
pub struct CudaRequest {
    pub version: Option<CudaVersion>,
}

impl CudaRequest {
    pub fn version(version: CudaVersion) -> Self {
        Self {
            version: Some(version),
        }
    }

    pub fn any() -> Self {
        Self { version: None }
    }
}

impl fmt::Display for CudaRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.version {
            Some(version) => write!(f, "CUDA {}", version),
            None => write!(f, "any CUDA version"),
        }
    }
}

impl FromStr for CudaRequest {
    type Err = CudaRequestError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() || s == "any" {
            return Ok(Self::any());
        }

        // try to parse as a version
        let version = s
            .parse::<CudaVersion>()
            .map_err(|_| CudaRequestError::InvalidRequest(s.to_string()))?;

        Ok(Self::version(version))
    }
}

#[derive(Debug, Error)]
pub enum CudaRequestError {
    #[error("Invalid CUDA request: `{0}`")]
    InvalidRequest(String),
}

/// a unique key for identifying CUDA installations.
#[derive(Debug, Clone, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CudaInstallationKey {
    pub version: CudaVersion,
    pub platform: String,
}

impl CudaInstallationKey {
    pub fn new(version: CudaVersion, platform: String) -> Self {
        Self { version, platform }
    }
}

impl fmt::Display for CudaInstallationKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "cuda-{}-{}", self.version, self.platform)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CudaSource {
    /// a managed CUDA installation (installed by uv).
    Managed,
    /// a system CUDA installation.
    System,
    /// an environment variable-specified CUDA installation.
    Environment,
}

impl fmt::Display for CudaSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Managed => write!(f, "managed"),
            Self::System => write!(f, "system"),
            Self::Environment => write!(f, "environment"),
        }
    }
}

pub fn find_cuda_installations(
    request: &CudaRequest,
) -> Result<Vec<(CudaSource, crate::installation::CudaInstallation)>, crate::Error> {
    let mut installations = Vec::new();

    // first, check for managed installations
    if let Ok(managed_installations) = ManagedCudaInstallations::from_settings(None) {
        if let Ok(all_installations) = managed_installations.find_all() {
            for installation in all_installations {
                if let Some(requested_version) = &request.version {
                    if !installation.version().matches(requested_version) {
                        continue;
                    }
                }

                installations.push((
                    CudaSource::Managed,
                    crate::installation::CudaInstallation::from_managed(installation),
                ));
            }
        }
    }

    // TODO(alpin): add system CUDA discovery (looking in standard paths like /usr/local/cuda)
    // TODO(alpin): add environment variable discovery (CUDA_HOME, CUDA_PATH, etc.)

    Ok(installations)
}
