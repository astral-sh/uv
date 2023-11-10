use std::fmt::{Display, Formatter};
use std::str::FromStr;

use thiserror::Error;

use pep440_rs::Version;
use puffin_normalize::{InvalidNameError, PackageName};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceDistExtension {
    Zip,
    TarGz,
}

impl FromStr for SourceDistExtension {
    type Err = SourceDistFilenameError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "zip" => Self::Zip,
            "tar.gz" => Self::TarGz,
            other => return Err(SourceDistFilenameError::InvalidExtension(other.to_string())),
        })
    }
}

impl Display for SourceDistExtension {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            SourceDistExtension::Zip => f.write_str("zip"),
            SourceDistExtension::TarGz => f.write_str("tar.gz"),
        }
    }
}

impl SourceDistExtension {
    pub fn from_filename(filename: &str) -> Option<(&str, Self)> {
        if let Some(stem) = filename.strip_suffix(".zip") {
            return Some((stem, Self::Zip));
        }
        if let Some(stem) = filename.strip_suffix(".tar.gz") {
            return Some((stem, Self::TarGz));
        }
        None
    }
}

/// Note that this is a normalized and not an exact representation, keep the original string if you
/// need the latter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceDistFilename {
    pub name: PackageName,
    pub version: Version,
    pub extension: SourceDistExtension,
}

impl SourceDistFilename {
    /// No `FromStr` impl since we need to know the package name to be able to reasonable parse
    /// these (consider e.g. `a-1-1.zip`)
    pub fn parse(
        filename: &str,
        package_name: &PackageName,
    ) -> Result<Self, SourceDistFilenameError> {
        let Some((stem, extension)) = SourceDistExtension::from_filename(filename) else {
            return Err(SourceDistFilenameError::InvalidExtension(
                filename.to_string(),
            ));
        };

        if stem.len() <= package_name.as_ref().len() + "-".len() {
            return Err(SourceDistFilenameError::InvalidFilename {
                filename: filename.to_string(),
                package_name: package_name.to_string(),
            });
        }
        let actual_package_name = PackageName::from_str(&stem[..package_name.as_ref().len()])
            .map_err(|err| {
                SourceDistFilenameError::InvalidPackageName(filename.to_string(), err)
            })?;
        if &actual_package_name != package_name {
            return Err(SourceDistFilenameError::InvalidFilename {
                filename: filename.to_string(),
                package_name: package_name.to_string(),
            });
        }

        // We checked the length above
        let version = Version::from_str(&stem[package_name.as_ref().len() + "-".len()..])
            .map_err(SourceDistFilenameError::InvalidVersion)?;

        Ok(Self {
            name: package_name.clone(),
            version,
            extension,
        })
    }
}

impl Display for SourceDistFilename {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}-{}.{}", self.name, self.version, self.extension)
    }
}

#[derive(Error, Debug, Clone)]
pub enum SourceDistFilenameError {
    #[error("Source distribution name {filename} doesn't start with package name {package_name}")]
    InvalidFilename {
        filename: String,
        package_name: String,
    },
    #[error("Source distributions filenames must end with .zip or .tar.gz, not {0}")]
    InvalidExtension(String),
    #[error("Source distribution filename version section is invalid: {0}")]
    InvalidVersion(String),
    #[error("Source distribution filename has an invalid package name: {0}")]
    InvalidPackageName(String, #[source] InvalidNameError),
}

#[cfg(test)]
mod tests {
    use puffin_normalize::PackageName;
    use std::str::FromStr;

    use crate::SourceDistFilename;

    /// Only test already normalized names since the parsing is lossy
    #[test]
    fn roundtrip() {
        for normalized in [
            "foo-lib-1.2.3.zip",
            "foo-lib-1.2.3a3.zip",
            "foo-lib-1.2.3.tar.gz",
        ] {
            assert_eq!(
                SourceDistFilename::parse(normalized, &PackageName::from_str("foo_lib").unwrap())
                    .unwrap()
                    .to_string(),
                normalized
            );
        }
    }

    #[test]
    fn errors() {
        for invalid in ["b-1.2.3.zip", "a-1.2.3-gamma.3.zip", "a-1.2.3.tar.zstd"] {
            assert!(
                SourceDistFilename::parse(invalid, &PackageName::from_str("a").unwrap()).is_err()
            );
        }
    }

    #[test]
    fn name_to_long() {
        assert!(
            SourceDistFilename::parse("foo.zip", &PackageName::from_str("foo-lib").unwrap())
                .is_err()
        );
    }
}
