use std::fmt::{Display, Formatter};
use std::path::Path;

use thiserror::Error;

use distribution_types::{ParsedUrl, ParsedUrlError, UvRequirement, UvSource};
use pep508_rs::{
    MarkerEnvironment, MarkerTree, Pep508Error, Pep508ErrorSource, UnnamedRequirement,
};
use uv_normalize::ExtraName;

use crate::Requirement;

/// A requirement specifier in a `requirements.txt` file.
#[derive(Hash, Debug, Clone, Eq, PartialEq)]
pub enum RequirementsTxtRequirement {
    /// The uv-specific superset over PEP 508 requirements specifier incorporating
    /// `tool.uv.sources`.
    Uv(UvRequirement),
    /// A PEP 508-like, direct URL dependency specifier.
    Unnamed(UnnamedRequirement),
}

impl Display for RequirementsTxtRequirement {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Uv(requirement) => write!(f, "{requirement}"),
            Self::Unnamed(requirement) => write!(f, "{requirement}"),
        }
    }
}

impl RequirementsTxtRequirement {
    /// For error messages.
    pub fn name_or_url(&self) -> String {
        match self {
            RequirementsTxtRequirement::Uv(requirement) => requirement.name.to_string(),
            RequirementsTxtRequirement::Unnamed(unnamed) => unnamed.to_string(),
        }
    }

    /// Returns whether the markers apply for the given environment
    pub fn evaluate_markers(&self, env: &MarkerEnvironment, extras: &[ExtraName]) -> bool {
        match self {
            Self::Uv(requirement) => requirement.evaluate_markers(env, extras),
            Self::Unnamed(requirement) => requirement.evaluate_markers(env, extras),
        }
    }

    /// Returns the extras for the requirement.
    pub fn extras(&self) -> &[ExtraName] {
        match self {
            Self::Uv(requirement) => requirement.extras.as_slice(),
            Self::Unnamed(requirement) => requirement.extras.as_slice(),
        }
    }

    /// Returns the markers for the requirement.
    pub fn markers(&self) -> Option<&MarkerTree> {
        match self {
            Self::Uv(requirement) => requirement.marker.as_ref(),
            Self::Unnamed(requirement) => requirement.marker.as_ref(),
        }
    }

    /// Return the version specifier or URL for the requirement.
    pub fn source(&self) -> UvSource {
        // TODO(konsti): Error handling
        match self {
            Self::Uv(requirement) => requirement.source.clone(),
            Self::Unnamed(requirement) => {
                let parsed_url = ParsedUrl::try_from(&requirement.url.to_url())
                    .expect("TODO(konsti): scheme not supported");
                UvSource::from_parsed_url(parsed_url, requirement.url.clone())
            }
        }
    }
}

impl From<UvRequirement> for RequirementsTxtRequirement {
    fn from(requirement: UvRequirement) -> Self {
        Self::Uv(requirement)
    }
}

impl From<UnnamedRequirement> for RequirementsTxtRequirement {
    fn from(requirement: UnnamedRequirement) -> Self {
        Self::Unnamed(requirement)
    }
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
        match Requirement::parse(input, &working_dir) {
            Ok(requirement) => Ok(Self::Uv(
                UvRequirement::from_requirement(requirement)
                    .map_err(|err| RequirementsTxtRequirementError::ParsedUrl(Box::new(err)))?,
            )),
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
