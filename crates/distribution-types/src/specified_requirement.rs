use std::borrow::Cow;
use std::fmt::{Display, Formatter};

use pep508_rs::{MarkerEnvironment, UnnamedRequirement};
use pypi_types::{Requirement, RequirementSource};
use uv_normalize::ExtraName;

use crate::VerbatimParsedUrl;

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
    Unnamed(UnnamedRequirement<VerbatimParsedUrl>),
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
    ///
    /// When the environment is not given, this treats all marker expressions
    /// that reference the environment as true. In other words, it does
    /// environment independent expression evaluation. (Which in turn devolves
    /// to "only evaluate marker expressions that reference an extra name.")
    pub fn evaluate_markers(&self, env: Option<&MarkerEnvironment>, extras: &[ExtraName]) -> bool {
        match self {
            Self::Named(requirement) => requirement.evaluate_markers(env, extras),
            Self::Unnamed(requirement) => requirement.evaluate_optional_environment(env, extras),
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
    pub fn source(&self) -> Cow<'_, RequirementSource> {
        match self {
            Self::Named(requirement) => Cow::Borrowed(&requirement.source),
            Self::Unnamed(requirement) => Cow::Owned(RequirementSource::from_parsed_url(
                requirement.url.parsed_url.clone(),
                requirement.url.verbatim.clone(),
            )),
        }
    }

    /// Returns `true` if the requirement is editable.
    pub fn is_editable(&self) -> bool {
        match self {
            Self::Named(requirement) => requirement.is_editable(),
            Self::Unnamed(requirement) => requirement.url.is_editable(),
        }
    }
}

impl From<Requirement> for UnresolvedRequirementSpecification {
    fn from(requirement: Requirement) -> Self {
        Self {
            requirement: UnresolvedRequirement::Named(requirement),
            hashes: Vec::new(),
        }
    }
}
