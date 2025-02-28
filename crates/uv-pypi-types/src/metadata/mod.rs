mod build_requires;
mod metadata10;
mod metadata23;
mod metadata_resolver;
mod pyproject_toml;
mod requires_dist;
mod requires_txt;

use std::str::Utf8Error;

use mailparse::{MailHeaderMap, MailParseError};
use thiserror::Error;

use uv_normalize::InvalidNameError;
use uv_pep440::{VersionParseError, VersionSpecifiersParseError};
use uv_pep508::Pep508Error;

use crate::VerbatimParsedUrl;

pub use build_requires::BuildRequires;
pub use metadata10::Metadata10;
pub use metadata23::Metadata23;
pub use metadata_resolver::ResolutionMetadata;
pub use pyproject_toml::PyProjectToml;
pub use requires_dist::RequiresDist;
pub use requires_txt::RequiresTxt;

/// <https://github.com/PyO3/python-pkginfo-rs/blob/d719988323a0cfea86d4737116d7917f30e819e2/src/error.rs>
///
/// The error type
#[derive(Error, Debug)]
pub enum MetadataError {
    #[error(transparent)]
    MailParse(#[from] MailParseError),
    #[error("Invalid `pyproject.toml`")]
    InvalidPyprojectTomlSyntax(#[source] toml_edit::TomlError),
    #[error(transparent)]
    InvalidPyprojectTomlSchema(toml_edit::de::Error),
    #[error("`pyproject.toml` is using the `[project]` table, but the required `project.name` field is not set")]
    MissingName,
    #[error("Metadata field {0} not found")]
    FieldNotFound(&'static str),
    #[error("Invalid version: {0}")]
    Pep440VersionError(VersionParseError),
    #[error(transparent)]
    Pep440Error(#[from] VersionSpecifiersParseError),
    #[error(transparent)]
    Pep508Error(#[from] Box<Pep508Error<VerbatimParsedUrl>>),
    #[error(transparent)]
    InvalidName(#[from] InvalidNameError),
    #[error("Invalid `Metadata-Version` field: {0}")]
    InvalidMetadataVersion(String),
    #[error("Reading metadata from `PKG-INFO` requires Metadata 2.2 or later (found: {0})")]
    UnsupportedMetadataVersion(String),
    #[error("The following field was marked as dynamic: {0}")]
    DynamicField(&'static str),
    #[error("The project uses Poetry's syntax to declare its dependencies, despite including a `project` table in `pyproject.toml`")]
    PoetrySyntax,
    #[error("Failed to read `requires.txt` contents")]
    RequiresTxtContents(#[from] std::io::Error),
    #[error("The description is not valid utf-8")]
    DescriptionEncoding(#[source] Utf8Error),
}

impl From<Pep508Error<VerbatimParsedUrl>> for MetadataError {
    fn from(error: Pep508Error<VerbatimParsedUrl>) -> Self {
        Self::Pep508Error(Box::new(error))
    }
}

/// The headers of a distribution metadata file.
#[derive(Debug)]
struct Headers<'a> {
    headers: Vec<mailparse::MailHeader<'a>>,
    body_start: usize,
}

impl<'a> Headers<'a> {
    /// Parse the headers from the given metadata file content.
    fn parse(content: &'a [u8]) -> Result<Self, MailParseError> {
        let (headers, body_start) = mailparse::parse_headers(content)?;
        Ok(Self {
            headers,
            body_start,
        })
    }

    /// Return the first value associated with the header with the given name.
    fn get_first_value(&self, name: &str) -> Option<String> {
        self.headers.get_first_header(name).and_then(|header| {
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
        self.headers
            .get_all_values(name)
            .into_iter()
            .filter(|value| value != "UNKNOWN")
    }
}

/// Parse a `Metadata-Version` field into a (major, minor) tuple.
fn parse_version(metadata_version: &str) -> Result<(u8, u8), MetadataError> {
    let (major, minor) =
        metadata_version
            .split_once('.')
            .ok_or(MetadataError::InvalidMetadataVersion(
                metadata_version.to_string(),
            ))?;
    let major = major
        .parse::<u8>()
        .map_err(|_| MetadataError::InvalidMetadataVersion(metadata_version.to_string()))?;
    let minor = minor
        .parse::<u8>()
        .map_err(|_| MetadataError::InvalidMetadataVersion(metadata_version.to_string()))?;
    Ok((major, minor))
}
