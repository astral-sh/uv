use std::fmt::{Display, Formatter};
use std::path::PathBuf;

use indexmap::IndexMap;
use url::Url;

use pep440_rs::VersionSpecifiers;
use pep508_rs::{MarkerEnvironment, MarkerTree, Requirement, VerbatimUrl, VersionOrUrl};
use uv_git::GitReference;
use uv_normalize::{ExtraName, PackageName};

use crate::{ParsedUrl, ParsedUrlError};

/// The requirements of a distribution, an extension over PEP 508's requirements.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct UvRequirements {
    pub dependencies: Vec<UvRequirement>,
    pub optional_dependencies: IndexMap<ExtraName, Vec<UvRequirement>>,
}

/// A representation of dependency on a package, an extension over a PEP 508's requirement.
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

/// The different kinds of requirements (version specifier, HTTP(S) URL, git repository, path).
#[derive(Hash, Debug, Clone, Eq, PartialEq)]
pub enum UvSource {
    /// The requirement has a version specifier, such as `foo >1,<2`.
    Registry {
        version: VersionSpecifiers,
        /// Choose a version from the index with this name.
        index: Option<String>,
    },
    // TODO(konsti): Track and verify version specifier from pyproject.toml
    /// A remote `http://` or `https://` URL, either a built distribution,
    /// e.g. `foo @ https://example.org/foo-1.0-py3-none-any.whl`, or a source distribution,
    /// e.g.`foo @ https://example.org/foo-1.0.zip`.
    Url {
        /// For source distributions, the location of the distribution if it is not in the archive
        /// root.
        subdirectory: Option<PathBuf>,
        url: VerbatimUrl,
    },
    /// A remote git repository, either over HTTPS or over SSH.
    Git {
        /// The repository URL (without `git+` prefix).
        repository: Url,
        /// Optionally, the revision, tag, or branch to use.
        reference: GitReference,
        /// The location of the distribution if it is not in the repository root.
        subdirectory: Option<PathBuf>,
        url: VerbatimUrl,
    },
    /// A local built or source distribution, either from a path or a `file://` URL. It can either
    /// be a binary distribution (a `.whl` file), a source distribution archive (a `.zip` or
    /// `.tag.gz` file) or a source tree (a directory with a pyproject.toml in, or a legacy
    /// source distribution with only a setup.py but non pyproject.toml in it).
    Path {
        path: PathBuf,
        /// For a source tree (a directory), whether to install as an editable.
        editable: Option<bool>,
        /// The `file://` URL representing the path.
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
