//! Derived from `pypi_types_crate`.

use std::io;
use std::str::FromStr;

use mailparse::{MailHeaderMap, MailParseError};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use pep440_rs::{Version, VersionParseError, VersionSpecifiers, VersionSpecifiersParseError};
use pep508_rs::{Pep508Error, Requirement};
use uv_normalize::{ExtraName, InvalidNameError, PackageName};

use crate::lenient_requirement::LenientRequirement;
use crate::{LenientExtraName, LenientVersionSpecifiers};

/// Python Package Metadata 2.1 as specified in
/// <https://packaging.python.org/specifications/core-metadata/>.
///
/// This is a subset of the full metadata specification, and only includes the
/// fields that are relevant to dependency resolution.
#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct Metadata21 {
    // Mandatory fields
    pub metadata_version: String,
    pub name: PackageName,
    pub version: Version,
    // Optional fields
    pub requires_dist: Vec<Requirement>,
    pub requires_python: Option<VersionSpecifiers>,
    pub provides_extras: Vec<ExtraName>,
}

/// <https://github.com/PyO3/python-pkginfo-rs/blob/d719988323a0cfea86d4737116d7917f30e819e2/src/error.rs>
///
/// The error type
#[derive(Error, Debug)]
pub enum Error {
    /// I/O error
    #[error(transparent)]
    Io(#[from] io::Error),
    /// mail parse error
    #[error(transparent)]
    MailParse(#[from] MailParseError),
    /// Metadata field not found
    #[error("metadata field {0} not found")]
    FieldNotFound(&'static str),
    /// Unknown distribution type
    #[error("unknown distribution type")]
    UnknownDistributionType,
    /// Metadata file not found
    #[error("metadata file not found")]
    MetadataNotFound,
    /// Invalid project URL (no comma)
    #[error("Invalid Project-URL field (missing comma): '{0}'")]
    InvalidProjectUrl(String),
    /// Multiple metadata files found
    #[error("found multiple metadata files: {0:?}")]
    MultipleMetadataFiles(Vec<String>),
    /// Invalid Version
    #[error("invalid version: {0}")]
    Pep440VersionError(VersionParseError),
    /// Invalid VersionSpecifier
    #[error(transparent)]
    Pep440Error(#[from] VersionSpecifiersParseError),
    /// Invalid Requirement
    #[error(transparent)]
    Pep508Error(#[from] Pep508Error),
    #[error(transparent)]
    InvalidName(#[from] InvalidNameError),
}

/// From <https://github.com/PyO3/python-pkginfo-rs/blob/d719988323a0cfea86d4737116d7917f30e819e2/src/metadata.rs#LL78C2-L91C26>
impl Metadata21 {
    /// Parse distribution metadata from metadata bytes
    pub fn parse(content: &[u8]) -> Result<Self, Error> {
        let (headers, _) = mailparse::parse_headers(content)?;

        let get_first_value = |name| {
            headers.get_first_header(name).and_then(|header| {
                let value = header.get_value();
                if value == "UNKNOWN" {
                    None
                } else {
                    Some(value)
                }
            })
        };
        let get_all_values = |name| {
            headers
                .get_all_values(name)
                .into_iter()
                .filter(|value| value != "UNKNOWN")
        };

        let metadata_version = headers
            .get_first_value("Metadata-Version")
            .ok_or(Error::FieldNotFound("Metadata-Version"))?;
        let name = PackageName::new(
            headers
                .get_first_value("Name")
                .ok_or(Error::FieldNotFound("Name"))?,
        )?;
        let version = Version::from_str(
            &headers
                .get_first_value("Version")
                .ok_or(Error::FieldNotFound("Version"))?,
        )
        .map_err(Error::Pep440VersionError)?;
        let requires_dist = get_all_values("Requires-Dist")
            .map(|requires_dist| {
                LenientRequirement::from_str(&requires_dist).map(Requirement::from)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let requires_python = get_first_value("Requires-Python")
            .map(|requires_python| {
                LenientVersionSpecifiers::from_str(&requires_python).map(VersionSpecifiers::from)
            })
            .transpose()?;
        let provides_extras = get_all_values("Provides-Extra")
            .filter_map(LenientExtraName::try_parse)
            .map(|result| match result {
                Ok(extra_name) => Ok(ExtraName::from(extra_name)),
                Err(err) => Err(err),
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Metadata21 {
            metadata_version,
            name,
            version,
            requires_dist,
            requires_python,
            provides_extras,
        })
    }
}

impl FromStr for Metadata21 {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Metadata21::parse(s.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use pep440_rs::Version;
    use uv_normalize::PackageName;

    use crate::Error;

    use super::Metadata21;

    #[test]
    fn test_parse_from_str() {
        let s = "Metadata-Version: 1.0";
        let meta: Result<Metadata21, Error> = s.parse();
        assert!(matches!(meta, Err(Error::FieldNotFound("Name"))));

        let s = "Metadata-Version: 1.0\nName: asdf";
        let meta = Metadata21::parse(s.as_bytes());
        assert!(matches!(meta, Err(Error::FieldNotFound("Version"))));

        let s = "Metadata-Version: 1.0\nName: asdf\nVersion: 1.0";
        let meta = Metadata21::parse(s.as_bytes()).unwrap();
        assert_eq!(meta.metadata_version, "1.0");
        assert_eq!(meta.name, PackageName::from_str("asdf").unwrap());
        assert_eq!(meta.version, Version::new([1, 0]));

        let s = "Metadata-Version: 1.0\nName: asdf\nVersion: 1.0\nAuthor: 中文\n\n一个 Python 包";
        let meta = Metadata21::parse(s.as_bytes()).unwrap();
        assert_eq!(meta.metadata_version, "1.0");
        assert_eq!(meta.name, PackageName::from_str("asdf").unwrap());
        assert_eq!(meta.version, Version::new([1, 0]));

        let s = "Metadata-Version: 1.0\nName: =?utf-8?q?foobar?=\nVersion: 1.0";
        let meta = Metadata21::parse(s.as_bytes()).unwrap();
        assert_eq!(meta.metadata_version, "1.0");
        assert_eq!(meta.name, PackageName::from_str("foobar").unwrap());
        assert_eq!(meta.version, Version::new([1, 0]));

        let s = "Metadata-Version: 1.0\nName: =?utf-8?q?=C3=A4_space?= <x@y.org>\nVersion: 1.0";
        let meta = Metadata21::parse(s.as_bytes());
        assert!(matches!(meta, Err(Error::InvalidName(_))));
    }
}
