use std::str::FromStr;
use tracing::debug;

use pep440_rs::Version;
use pep508_rs::{MarkerEnvironment, StringVersion};

#[derive(Debug, Clone)]
pub(crate) struct PythonVersion(StringVersion);

impl FromStr for PythonVersion {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let version = StringVersion::from_str(s)?;
        if version.is_dev() {
            return Err(format!("Python version {s} is a development release"));
        }
        if version.is_local() {
            return Err(format!("Python version {s} is a local version"));
        }
        if version.epoch() != 0 {
            return Err(format!("Python version {s} has a non-zero epoch"));
        }
        if version.version < Version::from_release(vec![3, 7]) {
            return Err(format!("Python version {s} must be >= 3.7"));
        }
        if version.version >= Version::from_release(vec![4, 0]) {
            return Err(format!("Python version {s} must be < 4.0"));
        }

        // If the version lacks a patch, assume the most recent known patch for that minor version.
        match version.release() {
            [3, 7] => {
                debug!("Assuming Python 3.7.17");
                Ok(Self(StringVersion::from_str("3.7.17")?))
            }
            [3, 8] => {
                debug!("Assuming Python 3.8.18");
                Ok(Self(StringVersion::from_str("3.8.18")?))
            }
            [3, 9] => {
                debug!("Assuming Python 3.9.18");
                Ok(Self(StringVersion::from_str("3.9.18")?))
            }
            [3, 10] => {
                debug!("Assuming Python 3.10.13");
                Ok(Self(StringVersion::from_str("3.10.13")?))
            }
            [3, 11] => {
                debug!("Assuming Python 3.11.6");
                Ok(Self(StringVersion::from_str("3.11.6")?))
            }
            [3, 12] => {
                debug!("Assuming Python 3.12.0");
                Ok(Self(StringVersion::from_str("3.12.0")?))
            }
            _ => Ok(Self(version)),
        }
    }
}

impl PythonVersion {
    /// Return a [`MarkerEnvironment`] compatible with the given [`PythonVersion`], based on
    /// a base [`MarkerEnvironment`].
    ///
    /// The returned [`MarkerEnvironment`] will preserve the base environment's platform markers,
    /// but override its Python version markers.
    pub(crate) fn markers(self, base: &MarkerEnvironment) -> MarkerEnvironment {
        let mut markers = base.clone();

        // Ex) `implementation_version == "3.12.0"`
        if markers.implementation_name == "cpython" {
            markers.implementation_version = self.0.clone();
        }

        // Ex) `python_full_version == "3.12.0"`
        markers.python_full_version = self.0.clone();

        // Ex) `python_version == "3.12"`
        markers.python_version = self.0;

        markers
    }
}
