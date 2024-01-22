use pep440_rs::Version;
use pep508_rs::{MarkerEnvironment, StringVersion};
use std::str::FromStr;

#[derive(Debug, Clone)]
pub struct PythonVersion(StringVersion);

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
        if version.version < Version::new([3, 7]) {
            return Err(format!("Python version {s} must be >= 3.7"));
        }
        if version.version >= Version::new([4, 0]) {
            return Err(format!("Python version {s} must be < 4.0"));
        }

        Ok(Self(version))
    }
}

impl PythonVersion {
    /// Return a [`MarkerEnvironment`] compatible with the given [`PythonVersion`], based on
    /// a base [`MarkerEnvironment`].
    ///
    /// The returned [`MarkerEnvironment`] will preserve the base environment's platform markers,
    /// but override its Python version markers.
    pub fn markers(self, base: &MarkerEnvironment) -> MarkerEnvironment {
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

    /// Return the full parsed Python version.
    pub fn version(&self) -> &Version {
        &self.0.version
    }

    /// Return the major version of this Python version.
    pub fn major(&self) -> u8 {
        u8::try_from(self.0.release()[0]).expect("invalid major version")
    }

    /// Return the minor version of this Python version.
    pub fn minor(&self) -> u8 {
        u8::try_from(self.0.release()[1]).expect("invalid minor version")
    }

    /// Return the patch version of this Python version, if set.
    pub fn patch(&self) -> Option<u8> {
        if let Some(patch) = self.0.release().get(2).cloned() {
            Some(u8::try_from(patch).expect("invalid patch version"))
        } else {
            None
        }
    }

    /// Returns a copy of the Python version without the patch version
    pub fn without_patch(&self) -> Self {
        Self::from_str(format!("{}.{}", self.major(), self.minor()).as_str())
            .expect("dropping a patch should always be valid")
    }
}
