use std::fmt::Display;
use std::path::Path;

use uv_normalize::PackageName;
use uv_pep508::{
    Pep508Error, Pep508ErrorSource, RequirementOrigin, TracingReporter, UnnamedRequirement,
    VersionOrUrl,
};
use uv_pypi_types::{MakeEditableError, VerbatimParsedUrl};

#[derive(Debug, thiserror::Error)]
pub enum EditableError {
    #[error("Editable `{0}` must refer to a local directory")]
    MissingVersion(PackageName),

    #[error("Editable `{0}` must refer to a local directory, not a versioned package")]
    Versioned(PackageName),

    #[error("Editable `{0}` must refer to a local directory, not an archive: `{1}`")]
    File(PackageName, String),

    #[error("Editable `{0}` must refer to a local directory, not an HTTPS URL: `{1}`")]
    Https(PackageName, String),

    #[error("Editable `{0}` must refer to a local directory, not a Git URL: `{1}`")]
    Git(PackageName, String),

    #[error("Editable must refer to a local directory, not an archive: `{0}`")]
    UnnamedFile(String),

    #[error("Editable must refer to a local directory, not an HTTPS URL: `{0}`")]
    UnnamedHttps(String),

    #[error("Editable must refer to a local directory, not a Git URL: `{0}`")]
    UnnamedGit(String),
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

    /// Convert the [`RequirementsTxtRequirement`] into an editable requirement.
    ///
    /// # Errors
    ///
    /// Returns [`EditableError`] if the requirement cannot be interpreted as editable.
    /// Specifically, only local directory URLs are supported.
    pub fn into_editable(self) -> Result<Self, EditableError> {
        match self {
            Self::Named(mut requirement) => {
                let Some(version_or_url) = requirement.version_or_url.as_mut() else {
                    return Err(EditableError::MissingVersion(requirement.name));
                };

                let VersionOrUrl::Url(url) = version_or_url else {
                    return Err(EditableError::Versioned(requirement.name));
                };

                if let Err(error) = url.make_editable() {
                    let display_url = url.to_string();
                    return Err(match error {
                        MakeEditableError::LocalArchive => {
                            EditableError::File(requirement.name, display_url)
                        }
                        MakeEditableError::RemoteArchive => {
                            EditableError::Https(requirement.name, display_url)
                        }
                        MakeEditableError::Git => EditableError::Git(requirement.name, display_url),
                    });
                }

                Ok(Self::Named(requirement))
            }
            Self::Unnamed(mut requirement) => {
                if let Err(error) = requirement.url.make_editable() {
                    let display_requirement = requirement.to_string();
                    return Err(match error {
                        MakeEditableError::LocalArchive => {
                            EditableError::UnnamedFile(display_requirement)
                        }
                        MakeEditableError::RemoteArchive => {
                            EditableError::UnnamedHttps(display_requirement)
                        }
                        MakeEditableError::Git => EditableError::UnnamedGit(display_requirement),
                    });
                }

                Ok(Self::Unnamed(requirement))
            }
        }
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
