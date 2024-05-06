use std::borrow::Cow;
use std::fmt::{Display, Formatter};

use pep508_rs::{MarkerEnvironment, UnnamedRequirement};
use uv_normalize::ExtraName;

use crate::{ParsedUrl, ParsedUrlError, Requirement, RequirementSource};

/// An [`UnresolvedRequirement`] with additional metadata from `requirements.txt`, currently only
/// hashes but in the future also editable and similar information.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct UnresolvedRequirementSpecification {
    /// The actual requirement.
    pub requirement: UnresolvedRequirement,
    /// Hashes of the downloadable packages.
    pub hashes: Vec<String>,
}

/// A requirement read from a `requirements.txt` or `pyproject.toml` file.
///
/// It is considered unresolved as we still need to query the URL for the `Unnamed` variant to
/// resolve the requirement name.
///
/// Analog to `RequirementsTxtRequirement` but with `distribution_types::Requirement` instead of
/// `pep508_rs::Requirement`.
#[derive(Hash, Debug, Clone, Eq, PartialEq)]
pub enum UnresolvedRequirement {
    /// The uv-specific superset over PEP 508 requirements specifier incorporating
    /// `tool.uv.sources`.
    Named(Requirement),
    /// A PEP 508-like, direct URL dependency specifier.
    Unnamed(UnnamedRequirement),
}

impl Display for UnresolvedRequirement {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Named(requirement) => write!(f, "{requirement}"),
            Self::Unnamed(requirement) => write!(f, "{requirement}"),
        }
    }
}

impl UnresolvedRequirement {
    /// Returns whether the markers apply for the given environment.
    pub fn evaluate_markers(&self, env: &MarkerEnvironment, extras: &[ExtraName]) -> bool {
        match self {
            Self::Named(requirement) => requirement.evaluate_markers(env, extras),
            Self::Unnamed(requirement) => requirement.evaluate_markers(env, extras),
        }
    }

    /// Returns the extras for the requirement.
    pub fn extras(&self) -> &[ExtraName] {
        match self {
            Self::Named(requirement) => requirement.extras.as_slice(),
            Self::Unnamed(requirement) => requirement.extras.as_slice(),
        }
    }

    /// Return the version specifier or URL for the requirement.
    pub fn source(&self) -> Result<Cow<'_, RequirementSource>, Box<ParsedUrlError>> {
        // TODO(konsti): This is a bad place to raise errors, we should have parsed the url earlier.
        match self {
            Self::Named(requirement) => Ok(Cow::Borrowed(&requirement.source)),
            Self::Unnamed(requirement) => {
                let parsed_url = ParsedUrl::try_from(requirement.url.to_url())?;
                Ok(Cow::Owned(RequirementSource::from_parsed_url(
                    parsed_url,
                    requirement.url.clone(),
                )))
            }
        }
    }
}
