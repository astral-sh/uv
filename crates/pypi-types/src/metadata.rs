//! Derived from `pypi_types_crate`.

use std::io;
use std::str::FromStr;

use mailparse::{MailHeaderMap, MailParseError};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::warn;

use pep440_rs::{Version, VersionParseError, VersionSpecifiers, VersionSpecifiersParseError};
use pep508_rs::{Pep508Error, Requirement};
use uv_normalize::{ExtraName, InvalidNameError, PackageName};

use crate::lenient_requirement::LenientRequirement;
use crate::LenientVersionSpecifiers;

/// Python Package Metadata 2.3 as specified in
/// <https://packaging.python.org/specifications/core-metadata/>.
///
/// This is a subset of the full metadata specification, and only includes the
/// fields that are relevant to dependency resolution.
///
/// At present, we support up to version 2.3 of the metadata specification.
#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct Metadata23 {
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
    #[error("Invalid `Metadata-Version` field: {0}")]
    InvalidMetadataVersion(String),
    #[error("Reading metadata from `PKG-INFO` requires Metadata 2.2 or later (found: {0})")]
    UnsupportedMetadataVersion(String),
    #[error("The following field was marked as dynamic: {0}")]
    DynamicField(&'static str),
}

/// From <https://github.com/PyO3/python-pkginfo-rs/blob/d719988323a0cfea86d4737116d7917f30e819e2/src/metadata.rs#LL78C2-L91C26>
impl Metadata23 {
    /// Parse the [`Metadata23`] from a `METADATA` file, as included in a built distribution (wheel).
    pub fn parse_metadata(content: &[u8]) -> Result<Self, Error> {
        let headers = Headers::parse(content)?;

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
        let requires_dist = headers
            .get_all_values("Requires-Dist")
            .map(|requires_dist| {
                LenientRequirement::from_str(&requires_dist).map(Requirement::from)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let requires_python = headers
            .get_first_value("Requires-Python")
            .map(|requires_python| {
                LenientVersionSpecifiers::from_str(&requires_python).map(VersionSpecifiers::from)
            })
            .transpose()?;
        let provides_extras = headers
            .get_all_values("Provides-Extra")
            .filter_map(|provides_extra| match ExtraName::new(provides_extra) {
                Ok(extra_name) => Some(extra_name),
                Err(err) => {
                    warn!("Ignoring invalid extra: {err}");
                    None
                }
            })
            .collect::<Vec<_>>();

        Ok(Self {
            metadata_version,
            name,
            version,
            requires_dist,
            requires_python,
            provides_extras,
        })
    }

    /// Read the [`Metadata23`] from a source distribution's `PKG-INFO` file, if it uses Metadata 2.2
    /// or later _and_ none of the required fields (`Requires-Python`, `Requires-Dist`, and
    /// `Provides-Extra`) are marked as dynamic.
    pub fn parse_pkg_info(content: &[u8]) -> Result<Self, Error> {
        let headers = Headers::parse(content)?;

        // To rely on a source distribution's `PKG-INFO` file, the `Metadata-Version` field must be
        // present and set to a value of at least `2.2`.
        let metadata_version = headers
            .get_first_value("Metadata-Version")
            .ok_or(Error::FieldNotFound("Metadata-Version"))?;

        // Parse the version into (major, minor).
        let (major, minor) = parse_version(&metadata_version)?;
        if (major, minor) < (2, 2) || (major, minor) >= (3, 0) {
            return Err(Error::UnsupportedMetadataVersion(metadata_version));
        }

        // If any of the fields we need are marked as dynamic, we can't use the `PKG-INFO` file.
        let dynamic = headers.get_all_values("Dynamic").collect::<Vec<_>>();
        for field in dynamic {
            match field.as_str() {
                "Requires-Python" => return Err(Error::DynamicField("Requires-Python")),
                "Requires-Dist" => return Err(Error::DynamicField("Requires-Dist")),
                "Provides-Extra" => return Err(Error::DynamicField("Provides-Extra")),
                _ => (),
            }
        }

        // The `Name` and `Version` fields are required, and can't be dynamic.
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

        // The remaining fields are required to be present.
        let requires_dist = headers
            .get_all_values("Requires-Dist")
            .map(|requires_dist| {
                LenientRequirement::from_str(&requires_dist).map(Requirement::from)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let requires_python = headers
            .get_first_value("Requires-Python")
            .map(|requires_python| {
                LenientVersionSpecifiers::from_str(&requires_python).map(VersionSpecifiers::from)
            })
            .transpose()?;
        let provides_extras = headers
            .get_all_values("Provides-Extra")
            .filter_map(|provides_extra| match ExtraName::new(provides_extra) {
                Ok(extra_name) => Some(extra_name),
                Err(err) => {
                    warn!("Ignoring invalid extra: {err}");
                    None
                }
            })
            .collect::<Vec<_>>();

        Ok(Self {
            metadata_version,
            name,
            version,
            requires_dist,
            requires_python,
            provides_extras,
        })
    }
}

/// Parse a `Metadata-Version` field into a (major, minor) tuple.
fn parse_version(metadata_version: &str) -> Result<(u8, u8), Error> {
    let (major, minor) = metadata_version
        .split_once('.')
        .ok_or(Error::InvalidMetadataVersion(metadata_version.to_string()))?;
    let major = major
        .parse::<u8>()
        .map_err(|_| Error::InvalidMetadataVersion(metadata_version.to_string()))?;
    let minor = minor
        .parse::<u8>()
        .map_err(|_| Error::InvalidMetadataVersion(metadata_version.to_string()))?;
    Ok((major, minor))
}

/// The headers of a distribution metadata file.
#[derive(Debug)]
struct Headers<'a>(Vec<mailparse::MailHeader<'a>>);

impl<'a> Headers<'a> {
    /// Parse the headers from the given metadata file content.
    fn parse(content: &'a [u8]) -> Result<Self, MailParseError> {
        let (headers, _) = mailparse::parse_headers(content)?;
        Ok(Self(headers))
    }

    /// Return the first value associated with the header with the given name.
    fn get_first_value(&self, name: &str) -> Option<String> {
        self.0.get_first_header(name).and_then(|header| {
            let value = header.get_value();
            if value == "UNKNOWN" {
                None
            } else {
                Some(value)
            }
        })
    }

    /// Return all values associated with the header with the given name.
    fn get_all_values(&self, name: &str) -> impl Iterator<Item = String> {
        self.0
            .get_all_values(name)
            .into_iter()
            .filter(|value| value != "UNKNOWN")
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use pep440_rs::Version;
    use uv_normalize::PackageName;

    use crate::Error;

    use super::Metadata23;

    #[test]
    fn test_parse_metadata() {
        let s = "Metadata-Version: 1.0";
        let meta = Metadata23::parse_metadata(s.as_bytes());
        assert!(matches!(meta, Err(Error::FieldNotFound("Name"))));

        let s = "Metadata-Version: 1.0\nName: asdf";
        let meta = Metadata23::parse_metadata(s.as_bytes());
        assert!(matches!(meta, Err(Error::FieldNotFound("Version"))));

        let s = "Metadata-Version: 1.0\nName: asdf\nVersion: 1.0";
        let meta = Metadata23::parse_metadata(s.as_bytes()).unwrap();
        assert_eq!(meta.metadata_version, "1.0");
        assert_eq!(meta.name, PackageName::from_str("asdf").unwrap());
        assert_eq!(meta.version, Version::new([1, 0]));

        let s = "Metadata-Version: 1.0\nName: asdf\nVersion: 1.0\nAuthor: 中文\n\n一个 Python 包";
        let meta = Metadata23::parse_metadata(s.as_bytes()).unwrap();
        assert_eq!(meta.metadata_version, "1.0");
        assert_eq!(meta.name, PackageName::from_str("asdf").unwrap());
        assert_eq!(meta.version, Version::new([1, 0]));

        let s = "Metadata-Version: 1.0\nName: =?utf-8?q?foobar?=\nVersion: 1.0";
        let meta = Metadata23::parse_metadata(s.as_bytes()).unwrap();
        assert_eq!(meta.metadata_version, "1.0");
        assert_eq!(meta.name, PackageName::from_str("foobar").unwrap());
        assert_eq!(meta.version, Version::new([1, 0]));

        let s = "Metadata-Version: 1.0\nName: =?utf-8?q?=C3=A4_space?= <x@y.org>\nVersion: 1.0";
        let meta = Metadata23::parse_metadata(s.as_bytes());
        assert!(matches!(meta, Err(Error::InvalidName(_))));
    }

    #[test]
    fn test_parse_pkg_info() {
        let s = "Metadata-Version: 2.1";
        let meta = Metadata23::parse_pkg_info(s.as_bytes());
        assert!(matches!(meta, Err(Error::UnsupportedMetadataVersion(_))));

        let s = "Metadata-Version: 2.2\nName: asdf";
        let meta = Metadata23::parse_pkg_info(s.as_bytes());
        assert!(matches!(meta, Err(Error::FieldNotFound("Version"))));

        let s = "Metadata-Version: 2.3\nName: asdf";
        let meta = Metadata23::parse_pkg_info(s.as_bytes());
        assert!(matches!(meta, Err(Error::FieldNotFound("Version"))));

        let s = "Metadata-Version: 2.3\nName: asdf\nVersion: 1.0";
        let meta = Metadata23::parse_pkg_info(s.as_bytes()).unwrap();
        assert_eq!(meta.metadata_version, "2.3");
        assert_eq!(meta.name, PackageName::from_str("asdf").unwrap());
        assert_eq!(meta.version, Version::new([1, 0]));

        let s = "Metadata-Version: 2.3\nName: asdf\nVersion: 1.0\nDynamic: Requires-Dist";
        let meta = Metadata23::parse_pkg_info(s.as_bytes()).unwrap_err();
        assert!(matches!(meta, Error::DynamicField("Requires-Dist")));

        let s = "Metadata-Version: 2.3\nName: asdf\nVersion: 1.0\nRequires-Dist: foo";
        let meta = Metadata23::parse_pkg_info(s.as_bytes()).unwrap();
        assert_eq!(meta.metadata_version, "2.3");
        assert_eq!(meta.name, PackageName::from_str("asdf").unwrap());
        assert_eq!(meta.version, Version::new([1, 0]));
        assert_eq!(meta.requires_dist, vec!["foo".parse().unwrap()]);
    }
}
