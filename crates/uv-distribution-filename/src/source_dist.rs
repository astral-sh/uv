use std::fmt::{Display, Formatter};
use std::str::FromStr;

use crate::SourceDistExtension;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uv_normalize::{InvalidNameError, PackageName};
use uv_pep440::{Version, VersionParseError};

/// Note that this is a normalized and not an exact representation, keep the original string if you
/// need the latter.
#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Deserialize,
    rkyv::Serialize,
)]
#[rkyv(derive(Debug))]
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
        extension: SourceDistExtension,
        package_name: &PackageName,
    ) -> Result<Self, SourceDistFilenameError> {
        // Drop the extension (e.g., given `tar.gz`, drop `.tar.gz`).
        if filename.len() <= extension.to_string().len() + 1 {
            return Err(SourceDistFilenameError {
                filename: filename.to_string(),
                kind: SourceDistFilenameErrorKind::Extension,
            });
        }

        let stem = &filename[..(filename.len() - (extension.to_string().len() + 1))];

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
        let Ok(extension) = SourceDistExtension::from_path(filename) else {
            return Err(SourceDistFilenameError {
                filename: filename.to_string(),
                kind: SourceDistFilenameErrorKind::Extension,
            });
        };

        // Drop the extension (e.g., given `tar.gz`, drop `.tar.gz`).
        if filename.len() <= extension.to_string().len() + 1 {
            return Err(SourceDistFilenameError {
                filename: filename.to_string(),
                kind: SourceDistFilenameErrorKind::Extension,
            });
        }

        let stem = &filename[..(filename.len() - (extension.to_string().len() + 1))];

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
        write!(
            f,
            "{}-{}.{}",
            self.name.as_dist_info_name(),
            self.version,
            self.extension
        )
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
    #[error("File extension is invalid")]
    Extension,
    #[error("Version section is invalid")]
    Version(#[from] VersionParseError),
    #[error(transparent)]
    PackageName(#[from] InvalidNameError),
    #[error("Missing name-version separator")]
    Minus,
}

#[cfg(test)]
mod tests;
