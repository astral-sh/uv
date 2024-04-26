use pep508_rs::{
    MarkerEnvironment, MarkerTree, Pep508Error, Pep508ErrorSource, Requirement, UnnamedRequirement,
    VersionOrUrl, VersionOrUrlRef,
};
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};
use std::path::Path;
use std::str::FromStr;
use uv_normalize::ExtraName;

/// A requirement specifier in a `requirements.txt` file.
#[derive(Hash, Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum RequirementsTxtRequirement {
    /// A PEP 508-compliant dependency specifier.
    Pep508(Requirement),
    /// A PEP 508-like, direct URL dependency specifier.
    Unnamed(UnnamedRequirement),
}

impl Display for RequirementsTxtRequirement {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pep508(requirement) => write!(f, "{requirement}"),
            Self::Unnamed(requirement) => write!(f, "{requirement}"),
        }
    }
}

impl RequirementsTxtRequirement {
    /// Returns whether the markers apply for the given environment
    pub fn evaluate_markers(&self, env: &MarkerEnvironment, extras: &[ExtraName]) -> bool {
        match self {
            Self::Pep508(requirement) => requirement.evaluate_markers(env, extras),
            Self::Unnamed(requirement) => requirement.evaluate_markers(env, extras),
        }
    }

    /// Returns the extras for the requirement.
    pub fn extras(&self) -> &[ExtraName] {
        match self {
            Self::Pep508(requirement) => requirement.extras.as_slice(),
            Self::Unnamed(requirement) => requirement.extras.as_slice(),
        }
    }

    /// Returns the markers for the requirement.
    pub fn markers(&self) -> Option<&MarkerTree> {
        match self {
            Self::Pep508(requirement) => requirement.marker.as_ref(),
            Self::Unnamed(requirement) => requirement.marker.as_ref(),
        }
    }

    /// Return the version specifier or URL for the requirement.
    pub fn version_or_url(&self) -> Option<VersionOrUrlRef> {
        match self {
            Self::Pep508(requirement) => match requirement.version_or_url.as_ref() {
                Some(VersionOrUrl::VersionSpecifier(specifiers)) => {
                    Some(VersionOrUrlRef::VersionSpecifier(specifiers))
                }
                Some(VersionOrUrl::Url(url)) => Some(VersionOrUrlRef::Url(url)),
                None => None,
            },
            Self::Unnamed(requirement) => Some(VersionOrUrlRef::Url(&requirement.url)),
        }
    }
}

impl From<Requirement> for RequirementsTxtRequirement {
    fn from(requirement: Requirement) -> Self {
        Self::Pep508(requirement)
    }
}

impl From<UnnamedRequirement> for RequirementsTxtRequirement {
    fn from(requirement: UnnamedRequirement) -> Self {
        Self::Unnamed(requirement)
    }
}

impl FromStr for RequirementsTxtRequirement {
    type Err = Pep508Error;

    /// Parse a requirement as seen in a `requirements.txt` file.
    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match Requirement::from_str(input) {
            Ok(requirement) => Ok(Self::Pep508(requirement)),
            Err(err) => match err.message {
                Pep508ErrorSource::UnsupportedRequirement(_) => {
                    Ok(Self::Unnamed(UnnamedRequirement::from_str(input)?))
                }
                _ => Err(err),
            },
        }
    }
}

impl RequirementsTxtRequirement {
    /// Parse a requirement as seen in a `requirements.txt` file.
    pub fn parse(input: &str, working_dir: impl AsRef<Path>) -> Result<Self, Pep508Error> {
        // Attempt to parse as a PEP 508-compliant requirement.
        match Requirement::parse(input, &working_dir) {
            Ok(requirement) => Ok(Self::Pep508(requirement)),
            Err(err) => match err.message {
                Pep508ErrorSource::UnsupportedRequirement(_) => {
                    // If that fails, attempt to parse as a direct URL requirement.
                    Ok(Self::Unnamed(UnnamedRequirement::parse(
                        input,
                        &working_dir,
                    )?))
                }
                _ => Err(err),
            },
        }
    }
}
