use std::fmt;

use uv_pep440::Version;

#[derive(Debug, Clone, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(tag = "name", rename_all = "lowercase")]
pub enum Accelerator {
    Cuda { driver_version: Version },
}

impl fmt::Display for Accelerator {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Cuda { driver_version } => write!(f, "CUDA {driver_version}"),
        }
    }
}
