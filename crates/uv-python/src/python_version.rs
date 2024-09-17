use std::fmt::{Display, Formatter};
use std::ops::Deref;
use std::str::FromStr;

use pep440_rs::Version;
use pep508_rs::{MarkerEnvironment, StringVersion};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PythonVersion(StringVersion);

impl Deref for PythonVersion {
    type Target = StringVersion;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl FromStr for PythonVersion {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let version = StringVersion::from_str(s)
            .map_err(|err| format!("Python version `{s}` could not be parsed: {err}"))?;
        if version.is_dev() {
            return Err(format!("Python version `{s}` is a development release"));
        }
        if version.is_local() {
            return Err(format!("Python version `{s}` is a local version"));
        }
        if version.epoch() != 0 {
            return Err(format!("Python version `{s}` has a non-zero epoch"));
        }

        Ok(Self(version))
    }
}

#[cfg(feature = "schemars")]
impl schemars::JsonSchema for PythonVersion {
    fn schema_name() -> String {
        String::from("PythonVersion")
    }

    fn json_schema(_gen: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
        schemars::schema::SchemaObject {
            instance_type: Some(schemars::schema::InstanceType::String.into()),
            string: Some(Box::new(schemars::schema::StringValidation {
                pattern: Some(r"^3\.\d+(\.\d+)?$".to_string()),
                ..schemars::schema::StringValidation::default()
            })),
            metadata: Some(Box::new(schemars::schema::Metadata {
                description: Some("A Python version specifier, e.g. `3.7` or `3.8.0`.".to_string()),
                ..schemars::schema::Metadata::default()
            })),
            ..schemars::schema::SchemaObject::default()
        }
        .into()
    }
}

impl<'de> serde::Deserialize<'de> for PythonVersion {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        PythonVersion::from_str(&s).map_err(serde::de::Error::custom)
    }
}

impl Display for PythonVersion {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.0, f)
    }
}

impl PythonVersion {
    /// Return a [`MarkerEnvironment`] compatible with the given [`PythonVersion`], based on
    /// a base [`MarkerEnvironment`].
    ///
    /// The returned [`MarkerEnvironment`] will preserve the base environment's platform markers,
    /// but override its Python version markers.
    pub fn markers(&self, base: &MarkerEnvironment) -> MarkerEnvironment {
        let mut markers = base.clone();

        // Ex) `implementation_version == "3.12.0"`
        if markers.implementation_name() == "cpython" {
            let python_full_version = self.python_full_version();
            markers = markers.with_implementation_version(StringVersion {
                // Retain the verbatim representation, provided by the user.
                string: self.0.to_string(),
                version: python_full_version,
            });
        }

        // Ex) `python_full_version == "3.12.0"`
        let python_full_version = self.python_full_version();
        markers = markers.with_python_full_version(StringVersion {
            // Retain the verbatim representation, provided by the user.
            string: self.0.to_string(),
            version: python_full_version,
        });

        // Ex) `python_version == "3.12"`
        let python_version = self.python_version();
        markers = markers.with_python_version(StringVersion {
            string: python_version.to_string(),
            version: python_version,
        });

        markers
    }

    /// Return the `python_version` marker corresponding to this Python version.
    ///
    /// This should include exactly a major and minor version, but no patch version.
    ///
    /// Ex) `python_version == "3.12"`
    pub fn python_version(&self) -> Version {
        let major = self.release().first().copied().unwrap_or(0);
        let minor = self.release().get(1).copied().unwrap_or(0);
        Version::new([major, minor])
    }

    /// Return the `python_full_version` marker corresponding to this Python version.
    ///
    /// This should include exactly a major, minor, and patch version (even if it's zero), along
    /// with any pre-release or post-release information.
    ///
    /// Ex) `python_full_version == "3.12.0b1"`
    pub fn python_full_version(&self) -> Version {
        let major = self.release().first().copied().unwrap_or(0);
        let minor = self.release().get(1).copied().unwrap_or(0);
        let patch = self.release().get(2).copied().unwrap_or(0);
        Version::new([major, minor, patch])
            .with_pre(self.0.pre())
            .with_post(self.0.post())
    }

    /// Return the full parsed Python version.
    pub fn version(&self) -> &Version {
        &self.0.version
    }

    /// Return the major version of this Python version.
    pub fn major(&self) -> u8 {
        u8::try_from(self.0.release().first().copied().unwrap_or(0)).expect("invalid major version")
    }

    /// Return the minor version of this Python version.
    pub fn minor(&self) -> u8 {
        u8::try_from(self.0.release().get(1).copied().unwrap_or(0)).expect("invalid minor version")
    }

    /// Return the patch version of this Python version, if set.
    pub fn patch(&self) -> Option<u8> {
        self.0
            .release()
            .get(2)
            .copied()
            .map(|patch| u8::try_from(patch).expect("invalid patch version"))
    }

    /// Returns a copy of the Python version without the patch version
    #[must_use]
    pub fn without_patch(&self) -> Self {
        Self::from_str(format!("{}.{}", self.major(), self.minor()).as_str())
            .expect("dropping a patch should always be valid")
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use pep440_rs::{Prerelease, PrereleaseKind, Version};

    use crate::PythonVersion;

    #[test]
    fn python_markers() {
        let version = PythonVersion::from_str("3.11.0").expect("valid python version");
        assert_eq!(version.python_version(), Version::new([3, 11]));
        assert_eq!(version.python_version().to_string(), "3.11");
        assert_eq!(version.python_full_version(), Version::new([3, 11, 0]));
        assert_eq!(version.python_full_version().to_string(), "3.11.0");

        let version = PythonVersion::from_str("3.11").expect("valid python version");
        assert_eq!(version.python_version(), Version::new([3, 11]));
        assert_eq!(version.python_version().to_string(), "3.11");
        assert_eq!(version.python_full_version(), Version::new([3, 11, 0]));
        assert_eq!(version.python_full_version().to_string(), "3.11.0");

        let version = PythonVersion::from_str("3.11.8a1").expect("valid python version");
        assert_eq!(version.python_version(), Version::new([3, 11]));
        assert_eq!(version.python_version().to_string(), "3.11");
        assert_eq!(
            version.python_full_version(),
            Version::new([3, 11, 8]).with_pre(Some(Prerelease {
                kind: PrereleaseKind::Alpha,
                number: 1
            }))
        );
        assert_eq!(version.python_full_version().to_string(), "3.11.8a1");
    }
}
