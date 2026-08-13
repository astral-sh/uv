use std::fmt::Display;
use std::path::Path;

use uv_errors::{Hint, Hints};
use uv_pep508::{
    Pep508Error, Pep508ErrorSource, RequirementOrigin, TracingReporter, UnnamedRequirement,
    VersionOrUrl,
};
use uv_pypi_types::VerbatimParsedUrl;

#[derive(Debug, thiserror::Error)]
pub enum MakeEditableError {
    #[error("Registry requirements cannot be editable")]
    Registry,

    #[error(transparent)]
    Url(#[from] uv_pypi_types::MakeEditableError),
}

impl Hint for MakeEditableError {
    fn hints(&self) -> Hints<'_> {
        Hints::from("Editable requirements must refer to a local directory")
    }
}

/// A requirement specifier in a `requirements.txt` file.
///
/// Analog to `UnresolvedRequirement` but with `uv_pep508::Requirement` instead of
/// `distribution_types::Requirement`.
#[derive(Hash, Debug, Clone, Eq, PartialEq)]
pub enum RequirementsTxtRequirement {
    /// The uv-specific superset over PEP 508 requirements specifier incorporating
    /// `tool.uv.sources`.
    Named(uv_pep508::Requirement<VerbatimParsedUrl>),
    /// A PEP 508-like, direct URL dependency specifier.
    Unnamed(UnnamedRequirement<VerbatimParsedUrl>),
}

impl RequirementsTxtRequirement {
    /// Set the source file containing the requirement.
    #[must_use]
    pub(crate) fn with_origin(self, origin: RequirementOrigin) -> Self {
        match self {
            Self::Named(requirement) => Self::Named(requirement.with_origin(origin)),
            Self::Unnamed(requirement) => Self::Unnamed(requirement.with_origin(origin)),
        }
    }

    /// Make the [`RequirementsTxtRequirement`] editable in place.
    ///
    /// # Errors
    ///
    /// Returns [`MakeEditableError`] if the requirement does not refer to a local directory.
    pub fn make_editable(&mut self) -> Result<(), MakeEditableError> {
        let url = match self {
            Self::Named(requirement) => {
                let Some(VersionOrUrl::Url(url)) = requirement.version_or_url.as_mut() else {
                    return Err(MakeEditableError::Registry);
                };
                url
            }
            Self::Unnamed(requirement) => &mut requirement.url,
        };

        url.make_editable()?;
        Ok(())
    }

    /// Parse a requirement as seen in a `requirements.txt` file.
    pub fn parse(
        input: &str,
        working_dir: impl AsRef<Path>,
        editable: bool,
    ) -> Result<Self, Box<Pep508Error<VerbatimParsedUrl>>> {
        // Attempt to parse as a PEP 508-compliant requirement.
        match uv_pep508::Requirement::parse(input, &working_dir) {
            Ok(requirement) => {
                // As a special-case, interpret `dagster` as `./dagster` if we're in editable mode.
                if editable && requirement.version_or_url.is_none() {
                    Ok(Self::Unnamed(UnnamedRequirement::parse(
                        input,
                        &working_dir,
                        &mut TracingReporter,
                    )?))
                } else {
                    Ok(Self::Named(requirement))
                }
            }
            Err(err) => match err.message {
                Pep508ErrorSource::UnsupportedRequirement(_) => {
                    // If that fails, attempt to parse as a direct URL requirement.
                    Ok(Self::Unnamed(UnnamedRequirement::parse(
                        input,
                        &working_dir,
                        &mut TracingReporter,
                    )?))
                }
                _ => Err(err),
            },
        }
        .map_err(Box::new)
    }
}

impl Display for RequirementsTxtRequirement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Named(requirement) => Display::fmt(&requirement, f),
            Self::Unnamed(requirement) => Display::fmt(&requirement, f),
        }
    }
}
