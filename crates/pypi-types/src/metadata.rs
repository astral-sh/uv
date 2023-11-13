//! Derived from `pypi_types_crate`.

use std::collections::HashMap;
use std::io;
use std::str::FromStr;

use mailparse::{MailHeaderMap, MailParseError};
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::warn;

use crate::lenient_requirement::LenientVersionSpecifiers;
use pep440_rs::{Pep440Error, Version, VersionSpecifiers};
use pep508_rs::{Pep508Error, Requirement};
use puffin_normalize::{ExtraName, InvalidNameError, PackageName};

/// Python Package Metadata 2.1 as specified in
/// <https://packaging.python.org/specifications/core-metadata/>
///
/// One addition is the requirements fixup which insert missing commas e.g. in
/// `elasticsearch-dsl (>=7.2.0<8.0.0)`
#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct Metadata21 {
    // Mandatory fields
    pub metadata_version: String,
    pub name: PackageName,
    pub version: Version,
    // Optional fields
    pub platforms: Vec<String>,
    pub supported_platforms: Vec<String>,
    pub summary: Option<String>,
    pub description: Option<String>,
    pub description_content_type: Option<String>,
    pub keywords: Option<String>,
    pub home_page: Option<String>,
    pub download_url: Option<String>,
    pub author: Option<String>,
    pub author_email: Option<String>,
    pub maintainer: Option<String>,
    pub maintainer_email: Option<String>,
    pub license: Option<String>,
    pub classifiers: Vec<String>,
    pub requires_dist: Vec<Requirement>,
    pub provides_dist: Vec<PackageName>,
    pub obsoletes_dist: Vec<String>,
    pub requires_python: Option<VersionSpecifiers>,
    pub requires_external: Vec<String>,
    pub project_urls: HashMap<String, String>,
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
    Pep440Error(#[from] Pep440Error),
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
            let values: Vec<String> = headers
                .get_all_values(name)
                .into_iter()
                .filter(|value| value != "UNKNOWN")
                .collect();
            values
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
        let platforms = get_all_values("Platform");
        let supported_platforms = get_all_values("Supported-Platform");
        let summary = get_first_value("Summary");
        let body = msg.get_body()?;
        let description = if body.trim().is_empty() {
            get_first_value("Description")
        } else {
            Some(body)
        };
        let keywords = get_first_value("Keywords");
        let home_page = get_first_value("Home-Page");
        let download_url = get_first_value("Download-URL");
        let author = get_first_value("Author");
        let author_email = get_first_value("Author-email");
        let license = get_first_value("License");
        let classifiers = get_all_values("Classifier");
        let requires_dist = get_all_values("Requires-Dist")
            .iter()
            .map(|requires_dist| LenientRequirement::from_str(requires_dist).map(Requirement::from))
            .collect::<Result<Vec<_>, _>>()?;
        let provides_dist = get_all_values("Provides-Dist")
            .into_iter()
            .map(PackageName::new)
            .collect::<Result<Vec<_>, _>>()?;
        let obsoletes_dist = get_all_values("Obsoletes-Dist");
        let maintainer = get_first_value("Maintainer");
        let maintainer_email = get_first_value("Maintainer-email");
        let requires_python = get_first_value("Requires-Python")
            .map(|requires_python| {
                LenientVersionSpecifiers::from_str(&requires_python).map(VersionSpecifiers::from)
            })
            .transpose()?;
        let requires_external = get_all_values("Requires-External");
        let project_urls = get_all_values("Project-URL")
            .iter()
            .map(|name_value| match name_value.split_once(',') {
                None => Err(Error::InvalidProjectUrl(name_value.clone())),
                Some((name, value)) => Ok((name.to_string(), value.trim().to_string())),
            })
            .collect::<Result<_, _>>()?;
        let provides_extras = get_all_values("Provides-Extra")
            .into_iter()
            .map(ExtraName::new)
            .collect::<Result<Vec<_>, _>>()?;
        let description_content_type = get_first_value("Description-Content-Type");
        Ok(Metadata21 {
            metadata_version,
            name,
            version,
            platforms,
            supported_platforms,
            summary,
            description,
            description_content_type,
            keywords,
            home_page,
            download_url,
            author,
            author_email,
            maintainer,
            maintainer_email,
            license,
            classifiers,
            requires_dist,
            provides_dist,
            obsoletes_dist,
            requires_python,
            requires_external,
            project_urls,
            provides_extras,
        })
    }
}

/// Ex) `>=7.2.0<8.0.0`
static MISSING_COMMA: Lazy<Regex> = Lazy::new(|| Regex::new(r"(\d)([<>=~^!])").unwrap());
/// Ex) `!=~5.0`
static NOT_EQUAL_TILDE: Lazy<Regex> = Lazy::new(|| Regex::new(r"!=~((?:\d\.)*\d)").unwrap());
/// Ex) `>=1.9.*`
static GREATER_THAN_STAR: Lazy<Regex> = Lazy::new(|| Regex::new(r">=(\d+\.\d+)\.\*").unwrap());
/// Ex) `!=3.0*`
static MISSING_DOT: Lazy<Regex> = Lazy::new(|| Regex::new(r"(\d\.\d)+\*").unwrap());
/// Ex) `>=3.6,`
static TRAILING_COMMA: Lazy<Regex> = Lazy::new(|| Regex::new(r",\)").unwrap());

/// Like [`Requirement`], but attempts to correct some common errors in user-provided requirements.
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
struct LenientRequirement(Requirement);

impl FromStr for LenientRequirement {
    type Err = Pep508Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match Requirement::from_str(s) {
            Ok(requirement) => Ok(Self(requirement)),
            Err(err) => {
                // Given `elasticsearch-dsl (>=7.2.0<8.0.0)`, rewrite to `elasticsearch-dsl (>=7.2.0,<8.0.0)`.
                let patched = MISSING_COMMA.replace_all(s, r"$1,$2");
                if patched != s {
                    if let Ok(requirement) = Requirement::from_str(&patched) {
                        warn!(
                        "Inserting missing comma into invalid requirement (before: `{s}`; after: `{patched}`)",
                    );
                        return Ok(Self(requirement));
                    }
                }

                // Given `jupyter-core (!=~5.0,>=4.12)`, rewrite to `jupyter-core (!=5.0.*,>=4.12)`.
                let patched = NOT_EQUAL_TILDE.replace_all(s, r"!=${1}.*");
                if patched != s {
                    if let Ok(requirement) = Requirement::from_str(&patched) {
                        warn!(
                        "Adding wildcard after invalid tilde operator (before: `{s}`; after: `{patched}`)",
                    );
                        return Ok(Self(requirement));
                    }
                }

                // Given `torch (>=1.9.*)`, rewrite to `torch (>=1.9)`.
                let patched = GREATER_THAN_STAR.replace_all(s, r">=${1}");
                if patched != s {
                    if let Ok(requirement) = Requirement::from_str(&patched) {
                        warn!(
                        "Removing star after greater equal operator (before: `{s}`; after: `{patched}`)",
                    );
                        return Ok(Self(requirement));
                    }
                }

                // Given `pyzmq (!=3.0*)`, rewrite to `pyzmq (!=3.0.*)`.
                let patched = MISSING_DOT.replace_all(s, r"${1}.*");
                if patched != s {
                    if let Ok(requirement) = Requirement::from_str(&patched) {
                        warn!(
                        "Inserting missing dot into invalid requirement (before: `{s}`; after: `{patched}`)",
                    );
                        return Ok(Self(requirement));
                    }
                }

                // Given `pyzmq (>=3.6,)`, rewrite to `pyzmq (>=3.6)`
                let patched = TRAILING_COMMA.replace_all(s, r")");
                if patched != s {
                    if let Ok(requirement) = Requirement::from_str(&patched) {
                        warn!(
                        "Removing trailing comma from invalid requirement (before: `{s}`; after: `{patched}`)",
                    );
                        return Ok(Self(requirement));
                    }
                }

                Err(err)
            }
        }
    }
}

impl From<LenientRequirement> for Requirement {
    fn from(requirement: LenientRequirement) -> Self {
        requirement.0
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use pep508_rs::Requirement;

    use super::LenientRequirement;

    #[test]
    fn missing_comma() {
        let actual: Requirement = LenientRequirement::from_str("elasticsearch-dsl (>=7.2.0<8.0.0)")
            .unwrap()
            .into();
        let expected: Requirement =
            Requirement::from_str("elasticsearch-dsl (>=7.2.0,<8.0.0)").unwrap();
        assert_eq!(actual, expected);
    }

    #[test]
    fn not_equal_tile() {
        let actual: Requirement = LenientRequirement::from_str("jupyter-core (!=~5.0,>=4.12)")
            .unwrap()
            .into();
        let expected: Requirement = Requirement::from_str("jupyter-core (!=5.0.*,>=4.12)").unwrap();
        assert_eq!(actual, expected);

        let actual: Requirement = LenientRequirement::from_str("jupyter-core (!=~5,>=4.12)")
            .unwrap()
            .into();
        let expected: Requirement = Requirement::from_str("jupyter-core (!=5.*,>=4.12)").unwrap();
        assert_eq!(actual, expected);
    }

    #[test]
    fn greater_than_star() {
        let actual: Requirement = LenientRequirement::from_str("torch (>=1.9.*)")
            .unwrap()
            .into();
        let expected: Requirement = Requirement::from_str("torch (>=1.9)").unwrap();
        assert_eq!(actual, expected);
    }

    #[test]
    fn missing_dot() {
        let actual: Requirement =
            LenientRequirement::from_str("pyzmq (>=2.7,!=3.0*,!=3.1*,!=3.2*)")
                .unwrap()
                .into();
        let expected: Requirement =
            Requirement::from_str("pyzmq (>=2.7,!=3.0.*,!=3.1.*,!=3.2.*)").unwrap();
        assert_eq!(actual, expected);
    }

    #[test]
    fn trailing_comma() {
        let actual: Requirement = LenientRequirement::from_str("pyzmq (>=3.6,)")
            .unwrap()
            .into();
        let expected: Requirement = Requirement::from_str("pyzmq (>=3.6)").unwrap();
        assert_eq!(actual, expected);
    }
}
