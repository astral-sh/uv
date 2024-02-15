use std::fmt::{Display, Formatter};
use std::str::FromStr;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
use thiserror::Error;

use pep440_rs::{Version, VersionParseError};
use uv_normalize::{InvalidNameError, PackageName};

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
    feature = "rkyv",
    derive(rkyv::Archive, rkyv::Deserialize, rkyv::Serialize)
)]
#[cfg_attr(feature = "rkyv", archive(check_bytes))]
#[cfg_attr(feature = "rkyv", archive_attr(derive(Debug)))]
pub enum SourceDistExtension {
    Zip,
    TarGz,
}

impl FromStr for SourceDistExtension {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "zip" => Self::Zip,
            "tar.gz" => Self::TarGz,
            other => return Err(other.to_string()),
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
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
    feature = "rkyv",
    derive(rkyv::Archive, rkyv::Deserialize, rkyv::Serialize)
)]
#[cfg_attr(feature = "rkyv", archive(check_bytes))]
#[cfg_attr(feature = "rkyv", archive_attr(derive(Debug)))]
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
            return Err(SourceDistFilenameError {
                filename: filename.to_string(),
                kind: SourceDistFilenameErrorKind::Extension,
            });
        };

        if stem.len() <= package_name.as_ref().len() + "-".len() {
            return Err(SourceDistFilenameError {
                filename: filename.to_string(),
                kind: SourceDistFilenameErrorKind::Filename(package_name.clone()),
            });
        }
        let actual_package_name = PackageName::from_str(&stem[..package_name.as_ref().len()])
            .map_err(|err| SourceDistFilenameError {
                filename: filename.to_string(),
                kind: SourceDistFilenameErrorKind::PackageName(err),
            })?;
        if &actual_package_name != package_name {
            return Err(SourceDistFilenameError {
                filename: filename.to_string(),
                kind: SourceDistFilenameErrorKind::Filename(package_name.clone()),
            });
        }

        // We checked the length above
        let version =
            Version::from_str(&stem[package_name.as_ref().len() + "-".len()..]).map_err(|err| {
                SourceDistFilenameError {
                    filename: filename.to_string(),
                    kind: SourceDistFilenameErrorKind::Version(err),
                }
            })?;

        Ok(Self {
            name: package_name.clone(),
            version,
            extension,
        })
    }

    /// Like [`SourceDistFilename::parse`], but without knowing the package name.
    ///
    /// Source dist filenames can be ambiguous, e.g. `a-1-1.tar.gz`. Without knowing the package name, we assume that
    /// source dist filename version doesn't contain minus (the version is normalized).
    pub fn parsed_normalized_filename(filename: &str) -> Result<Self, SourceDistFilenameError> {
        let Some((stem, extension)) = SourceDistExtension::from_filename(filename) else {
            return Err(SourceDistFilenameError {
                filename: filename.to_string(),
                kind: SourceDistFilenameErrorKind::Extension,
            });
        };

        let Some((package_name, version)) = stem.rsplit_once('-') else {
            return Err(SourceDistFilenameError {
                filename: filename.to_string(),
                kind: SourceDistFilenameErrorKind::Minus,
            });
        };
        let package_name =
            PackageName::from_str(package_name).map_err(|err| SourceDistFilenameError {
                filename: filename.to_string(),
                kind: SourceDistFilenameErrorKind::PackageName(err),
            })?;

        // We checked the length above
        let version = Version::from_str(version).map_err(|err| SourceDistFilenameError {
            filename: filename.to_string(),
            kind: SourceDistFilenameErrorKind::Version(err),
        })?;

        Ok(Self {
            name: package_name,
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
pub struct SourceDistFilenameError {
    filename: String,
    kind: SourceDistFilenameErrorKind,
}

impl Display for SourceDistFilenameError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Failed to parse source distribution filename {}: {}",
            self.filename, self.kind
        )
    }
}

#[derive(Error, Debug, Clone)]
enum SourceDistFilenameErrorKind {
    #[error("Name doesn't start with package name {0}")]
    Filename(PackageName),
    #[error("Source distributions filenames must end with .zip or .tar.gz")]
    Extension,
    #[error("Version section is invalid")]
    Version(#[from] VersionParseError),
    #[error(transparent)]
    PackageName(#[from] InvalidNameError),
    #[error("Missing name-version separator")]
    Minus,
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use uv_normalize::PackageName;

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
