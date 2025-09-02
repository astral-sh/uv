use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct CudaVersion {
    major: u8,
    minor: u8,
    patch: Option<u8>,
}

#[derive(Debug, Error)]
pub enum CudaVersionError {
    #[error("Invalid CUDA version: `{0}`")]
    InvalidVersion(String),
}

impl CudaVersion {
    pub fn new(major: u8, minor: u8, patch: Option<u8>) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }

    /// major version
    pub fn major(&self) -> u8 {
        self.major
    }

    /// minor version
    pub fn minor(&self) -> u8 {
        self.minor
    }

    /// patch version, if any
    pub fn patch(&self) -> Option<u8> {
        self.patch
    }

    /// version without the patch component
    pub fn without_patch(&self) -> CudaVersion {
        CudaVersion {
            major: self.major,
            minor: self.minor,
            patch: None,
        }
    }

    /// `true` if this version matches the given version request
    pub fn matches(&self, other: &CudaVersion) -> bool {
        if self.major != other.major {
            return false;
        }
        if self.minor != other.minor {
            return false;
        }
        match (self.patch, other.patch) {
            (Some(patch1), Some(patch2)) => patch1 == patch2,
            (None, _) | (_, None) => true,
        }
    }
}

impl fmt::Display for CudaVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.patch {
            Some(patch) => write!(f, "{}.{}.{}", self.major, self.minor, patch),
            None => write!(f, "{}.{}", self.major, self.minor),
        }
    }
}

impl FromStr for CudaVersion {
    type Err = CudaVersionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.split('.').collect();
        match parts.as_slice() {
            [major] => {
                let major = major
                    .parse::<u8>()
                    .map_err(|_| CudaVersionError::InvalidVersion(s.to_string()))?;
                Ok(CudaVersion::new(major, 0, None))
            }
            [major, minor] => {
                let major = major
                    .parse::<u8>()
                    .map_err(|_| CudaVersionError::InvalidVersion(s.to_string()))?;
                let minor = minor
                    .parse::<u8>()
                    .map_err(|_| CudaVersionError::InvalidVersion(s.to_string()))?;
                Ok(CudaVersion::new(major, minor, None))
            }
            [major, minor, patch] => {
                let major = major
                    .parse::<u8>()
                    .map_err(|_| CudaVersionError::InvalidVersion(s.to_string()))?;
                let minor = minor
                    .parse::<u8>()
                    .map_err(|_| CudaVersionError::InvalidVersion(s.to_string()))?;
                let patch = patch
                    .parse::<u8>()
                    .map_err(|_| CudaVersionError::InvalidVersion(s.to_string()))?;
                Ok(CudaVersion::new(major, minor, Some(patch)))
            }
            _ => Err(CudaVersionError::InvalidVersion(s.to_string())),
        }
    }
}
