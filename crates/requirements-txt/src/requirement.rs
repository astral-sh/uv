use std::path::Path;

use pep508_rs::{
    Pep508Error, Pep508ErrorSource, RequirementOrigin, TracingReporter, UnnamedRequirement,
};
use pypi_types::{ParsedDirectoryUrl, ParsedUrl, VerbatimParsedUrl};
use uv_normalize::PackageName;

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
/// Analog to `UnresolvedRequirement` but with `pep508_rs::Requirement` instead of
/// `distribution_types::Requirement`.
#[derive(Hash, Debug, Clone, Eq, PartialEq)]
pub enum RequirementsTxtRequirement {
    /// The uv-specific superset over PEP 508 requirements specifier incorporating
    /// `tool.uv.sources`.
    Named(pep508_rs::Requirement<VerbatimParsedUrl>),
    /// A PEP 508-like, direct URL dependency specifier.
    Unnamed(UnnamedRequirement<VerbatimParsedUrl>),
}

impl RequirementsTxtRequirement {
    /// Set the source file containing the requirement.
    #[must_use]
    pub fn with_origin(self, origin: RequirementOrigin) -> Self {
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
            RequirementsTxtRequirement::Named(requirement) => {
                let Some(version_or_url) = requirement.version_or_url else {
                    return Err(EditableError::MissingVersion(requirement.name));
                };

                let pep508_rs::VersionOrUrl::Url(url) = version_or_url else {
                    return Err(EditableError::Versioned(requirement.name));
                };

                let parsed_url = match url.parsed_url {
                    ParsedUrl::Directory(parsed_url) => parsed_url,
                    ParsedUrl::Path(_) => {
                        return Err(EditableError::File(requirement.name, url.to_string()))
                    }
                    ParsedUrl::Archive(_) => {
                        return Err(EditableError::Https(requirement.name, url.to_string()))
                    }
                    ParsedUrl::Git(_) => {
                        return Err(EditableError::Git(requirement.name, url.to_string()))
                    }
                };

                Ok(Self::Named(pep508_rs::Requirement {
                    version_or_url: Some(pep508_rs::VersionOrUrl::Url(VerbatimParsedUrl {
                        verbatim: url.verbatim,
                        parsed_url: ParsedUrl::Directory(ParsedDirectoryUrl {
                            editable: true,
                            ..parsed_url
                        }),
                    })),
                    ..requirement
                }))
            }
            RequirementsTxtRequirement::Unnamed(requirement) => {
                let parsed_url = match requirement.url.parsed_url {
                    ParsedUrl::Directory(parsed_url) => parsed_url,
                    ParsedUrl::Path(_) => {
                        return Err(EditableError::UnnamedFile(requirement.to_string()))
                    }
                    ParsedUrl::Archive(_) => {
                        return Err(EditableError::UnnamedHttps(requirement.to_string()))
                    }
                    ParsedUrl::Git(_) => {
                        return Err(EditableError::UnnamedGit(requirement.to_string()))
                    }
                };

                Ok(Self::Unnamed(UnnamedRequirement {
                    url: VerbatimParsedUrl {
                        verbatim: requirement.url.verbatim,
                        parsed_url: ParsedUrl::Directory(ParsedDirectoryUrl {
                            editable: true,
                            ..parsed_url
                        }),
                    },
                    ..requirement
                }))
            }
        }
    }
}

impl RequirementsTxtRequirement {
    /// Parse a requirement as seen in a `requirements.txt` file.
    pub fn parse(
        input: &str,
        working_dir: impl AsRef<Path>,
        editable: bool,
    ) -> Result<Self, Box<Pep508Error<VerbatimParsedUrl>>> {
        // Attempt to parse as a PEP 508-compliant requirement.
        match pep508_rs::Requirement::parse(input, &working_dir) {
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
