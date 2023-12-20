//! Derived from `pypi_types_crate`.

use std::io;
use std::str::FromStr;

use mailparse::{MailHeaderMap, MailParseError};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use pep440_rs::{Version, VersionSpecifiers, VersionSpecifiersParseError};
use pep508_rs::{Pep508Error, Requirement};
use puffin_normalize::{ExtraName, InvalidNameError, PackageName};

use crate::lenient_requirement::LenientRequirement;
use crate::LenientVersionSpecifiers;

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
    Pep440VersionError(String),
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
        // HACK: trick mailparse to parse as UTF-8 instead of ASCII
        let mut mail = b"Content-Type: text/plain; charset=utf-8\n".to_vec();
        mail.extend_from_slice(content);

        let msg = mailparse::parse_mail(&mail)?;
        let headers = msg.get_headers();

        let get_first_value = |name| {
            headers.get_first_header(name).and_then(|header| {
                match rfc2047_decoder::decode(header.get_value_raw()) {
                    Ok(value) => {
                        if value == "UNKNOWN" {
                            None
                        } else {
                            Some(value)
                        }
                    }
                    Err(_) => None,
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
            .map(ExtraName::new)
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
