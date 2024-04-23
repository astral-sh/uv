use std::fmt::{Display, Formatter};
use std::path::PathBuf;

use indexmap::IndexMap;
use url::Url;

use pep440_rs::VersionSpecifiers;
use pep508_rs::{MarkerEnvironment, MarkerTree, Requirement, VerbatimUrl, VersionOrUrl};
use uv_git::GitReference;
use uv_normalize::{ExtraName, PackageName};

use crate::{ParsedUrl, ParsedUrlError};

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct UvRequirements {
    pub dependencies: Vec<UvRequirement>,
    pub optional_dependencies: IndexMap<ExtraName, Vec<UvRequirement>>,
}

#[derive(Hash, Debug, Clone, Eq, PartialEq)]
pub struct UvRequirement {
    pub name: PackageName,
    pub extras: Vec<ExtraName>,
    pub marker: Option<MarkerTree>,
    pub source: UvSource,
}

impl UvRequirement {
    /// Returns whether the markers apply for the given environment.
    pub fn evaluate_markers(&self, env: &MarkerEnvironment, extras: &[ExtraName]) -> bool {
        if let Some(marker) = &self.marker {
            marker.evaluate(env, extras)
        } else {
            true
        }
    }

    pub fn from_requirement(requirement: Requirement) -> Result<Self, ParsedUrlError> {
        let source = match requirement.version_or_url {
            None => UvSource::Registry {
                version: VersionSpecifiers::empty(),
                index: None,
            },
            // The most popular case: Just a name, a version range and maybe extras.
            Some(VersionOrUrl::VersionSpecifier(version)) => UvSource::Registry {
                version,
                index: None,
            },
            Some(VersionOrUrl::Url(url)) => {
                let direct_url = ParsedUrl::try_from(&url.to_url())?;
                UvSource::from_parsed_url(direct_url, url)
            }
        };
        Ok(UvRequirement {
            name: requirement.name,
            extras: requirement.extras,
            marker: requirement.marker,
            source,
        })
    }
}

impl Display for UvRequirement {
    /// Note: This is for user display, not for requirements.txt
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)?;
        if !self.extras.is_empty() {
            write!(
                f,
                "[{}]",
                self.extras
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(",")
            )?;
        }
        match &self.source {
            UvSource::Registry { version, index } => {
                write!(f, "{version}")?;
                if let Some(index) = index {
                    write!(f, " (index: {index})")?;
                }
            }
            UvSource::Url { url, .. } => {
                write!(f, " @ {url}")?;
            }
            UvSource::Git {
                url: _,
                repository,
                reference,
                subdirectory,
            } => {
                write!(f, " @ git+{repository}")?;
                if let Some(reference) = reference.as_str() {
                    write!(f, "@{reference}")?;
                }
                if let Some(subdirectory) = subdirectory {
                    writeln!(f, "#subdirectory={}", subdirectory.display())?;
                }
            }
            UvSource::Path { url, .. } => {
                write!(f, " @ {url}")?;
            }
        }
        if let Some(marker) = &self.marker {
            write!(f, " ; {marker}")?;
        }
        Ok(())
    }
}

#[derive(Hash, Debug, Clone, Eq, PartialEq)]
pub enum UvSource {
    Registry {
        version: VersionSpecifiers,
        index: Option<String>,
    },
    // TODO(konsti): Track and verify version specifier from pyproject.toml
    Url {
        subdirectory: Option<PathBuf>,
        url: VerbatimUrl,
    },
    Git {
        repository: Url,
        reference: GitReference,
        subdirectory: Option<PathBuf>,
        url: VerbatimUrl,
    },
    Path {
        path: PathBuf,
        editable: Option<bool>,
        url: VerbatimUrl,
    },
}

impl UvSource {
    pub fn from_parsed_url(parsed_url: ParsedUrl, url: VerbatimUrl) -> Self {
        match parsed_url {
            ParsedUrl::LocalFile(local_file) => UvSource::Path {
                path: local_file.path,
                url,
                editable: None,
            },
            ParsedUrl::Git(git) => UvSource::Git {
                url,
                repository: git.url.repository().clone(),
                reference: git.url.reference().clone(),
                subdirectory: git.subdirectory,
            },
            ParsedUrl::Archive(archive) => UvSource::Url {
                url,
                subdirectory: archive.subdirectory,
            },
        }
    }
}
