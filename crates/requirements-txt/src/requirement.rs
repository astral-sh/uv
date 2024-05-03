use std::path::Path;

use thiserror::Error;

use distribution_types::ParsedUrlError;
use pep508_rs::{Pep508Error, Pep508ErrorSource, UnnamedRequirement};

/// A requirement specifier in a `requirements.txt` file.
///
/// Analog to `SpecifiedRequirement` but with `pep508_rs::Requirement` instead of
/// `distribution_types::Requirement`.
#[derive(Hash, Debug, Clone, Eq, PartialEq)]
pub enum RequirementsTxtRequirement {
    /// The uv-specific superset over PEP 508 requirements specifier incorporating
    /// `tool.uv.sources`.
    Named(pep508_rs::Requirement),
    /// A PEP 508-like, direct URL dependency specifier.
    Unnamed(UnnamedRequirement),
}

#[derive(Debug, Error)]
pub enum RequirementsTxtRequirementError {
    #[error(transparent)]
    ParsedUrl(#[from] Box<ParsedUrlError>),
    #[error(transparent)]
    Pep508(#[from] Pep508Error),
}

impl RequirementsTxtRequirement {
    /// Parse a requirement as seen in a `requirements.txt` file.
    pub fn parse(
        input: &str,
        working_dir: impl AsRef<Path>,
    ) -> Result<Self, RequirementsTxtRequirementError> {
        // Attempt to parse as a PEP 508-compliant requirement.
        match pep508_rs::Requirement::parse(input, &working_dir) {
            Ok(requirement) => Ok(Self::Named(requirement)),
            Err(err) => match err.message {
                Pep508ErrorSource::UnsupportedRequirement(_) => {
                    // If that fails, attempt to parse as a direct URL requirement.
                    Ok(Self::Unnamed(UnnamedRequirement::parse(
                        input,
                        &working_dir,
                    )?))
                }
                _ => Err(RequirementsTxtRequirementError::Pep508(err)),
            },
        }
    }
}
