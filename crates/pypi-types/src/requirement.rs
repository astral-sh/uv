use std::fmt::{Display, Formatter};
use std::path::PathBuf;

use url::Url;

use pep440_rs::VersionSpecifiers;
use pep508_rs::{MarkerEnvironment, MarkerTree, RequirementOrigin, VerbatimUrl, VersionOrUrl};
use uv_git::{GitReference, GitSha};
use uv_normalize::{ExtraName, PackageName};

use crate::{ParsedUrl, VerbatimParsedUrl};

/// A representation of dependency on a package, an extension over a PEP 508's requirement.
///
/// The main change is using [`RequirementSource`] to represent all supported package sources over
/// [`VersionOrUrl`], which collapses all URL sources into a single stringly type.
#[derive(Hash, Debug, Clone, Eq, PartialEq)]
pub struct Requirement {
    pub name: PackageName,
    pub extras: Vec<ExtraName>,
    pub marker: Option<MarkerTree>,
    pub source: RequirementSource,
    pub origin: Option<RequirementOrigin>,
}

impl Requirement {
    /// Returns whether the markers apply for the given environment.
    ///
    /// When `env` is `None`, this specifically evaluates all marker
    /// expressions based on the environment to `true`. That is, this provides
    /// environment independent marker evaluation.
    pub fn evaluate_markers(&self, env: Option<&MarkerEnvironment>, extras: &[ExtraName]) -> bool {
        if let Some(marker) = &self.marker {
            marker.evaluate_optional_environment(env, extras)
        } else {
            true
        }
    }

    /// Returns `true` if the requirement is editable.
    pub fn is_editable(&self) -> bool {
        self.source.is_editable()
    }
}

impl From<pep508_rs::Requirement<VerbatimParsedUrl>> for Requirement {
    /// Convert a [`pep508_rs::Requirement`] to a [`Requirement`].
    fn from(requirement: pep508_rs::Requirement<VerbatimParsedUrl>) -> Self {
        let source = match requirement.version_or_url {
            None => RequirementSource::Registry {
                specifier: VersionSpecifiers::empty(),
                index: None,
            },
            // The most popular case: just a name, a version range and maybe extras.
            Some(VersionOrUrl::VersionSpecifier(specifier)) => RequirementSource::Registry {
                specifier,
                index: None,
            },
            Some(VersionOrUrl::Url(url)) => {
                RequirementSource::from_parsed_url(url.parsed_url, url.verbatim)
            }
        };
        Requirement {
            name: requirement.name,
            extras: requirement.extras,
            marker: requirement.marker,
            source,
            origin: requirement.origin,
        }
    }
}

impl Display for Requirement {
    /// Display the [`Requirement`], with the intention of being shown directly to a user, rather
    /// than for inclusion in a `requirements.txt` file.
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
            RequirementSource::Registry { specifier, index } => {
                write!(f, "{specifier}")?;
                if let Some(index) = index {
                    write!(f, " (index: {index})")?;
                }
            }
            RequirementSource::Url { url, .. } => {
                write!(f, " @ {url}")?;
            }
            RequirementSource::Git {
                url: _,
                repository,
                reference,
                precise: _,
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
            RequirementSource::Path { url, .. } => {
                write!(f, " @ {url}")?;
            }
        }
        if let Some(marker) = &self.marker {
            write!(f, " ; {marker}")?;
        }
        Ok(())
    }
}

/// The different locations with can install a distribution from: Version specifier (from an index),
/// HTTP(S) URL, git repository, and path.
///
/// We store both the parsed fields (such as the plain url and the subdirectory) and the joined
/// PEP 508 style url (e.g. `file:///<path>#subdirectory=<subdirectory>`) since we need both in
/// different locations.
#[derive(Hash, Debug, Clone, Eq, PartialEq)]
pub enum RequirementSource {
    /// The requirement has a version specifier, such as `foo >1,<2`.
    Registry {
        specifier: VersionSpecifiers,
        /// Choose a version from the index with this name.
        index: Option<String>,
    },
    // TODO(konsti): Track and verify version specifier from `project.dependencies` matches the
    // version in remote location.
    /// A remote `http://` or `https://` URL, either a built distribution,
    /// e.g. `foo @ https://example.org/foo-1.0-py3-none-any.whl`, or a source distribution,
    /// e.g.`foo @ https://example.org/foo-1.0.zip`.
    Url {
        /// For source distributions, the path to the distribution if it is not in the archive
        /// root.
        subdirectory: Option<PathBuf>,
        /// The remote location of the archive file, without subdirectory fragment.
        location: Url,
        /// The PEP 508 style URL in the format
        /// `<scheme>://<domain>/<path>#subdirectory=<subdirectory>`.
        url: VerbatimUrl,
    },
    /// A remote Git repository, over either HTTPS or SSH.
    Git {
        /// The repository URL (without the `git+` prefix).
        repository: Url,
        /// Optionally, the revision, tag, or branch to use.
        reference: GitReference,
        /// The precise commit to use, if known.
        precise: Option<GitSha>,
        /// The path to the source distribution if it is not in the repository root.
        subdirectory: Option<PathBuf>,
        /// The PEP 508 style url in the format
        /// `git+<scheme>://<domain>/<path>@<rev>#subdirectory=<subdirectory>`.
        url: VerbatimUrl,
    },
    /// A local built or source distribution, either from a path or a `file://` URL. It can either
    /// be a binary distribution (a `.whl` file), a source distribution archive (a `.zip` or
    /// `.tag.gz` file) or a source tree (a directory with a pyproject.toml in, or a legacy
    /// source distribution with only a setup.py but non pyproject.toml in it).
    Path {
        path: PathBuf,
        /// For a source tree (a directory), whether to install as an editable.
        editable: bool,
        /// The PEP 508 style URL in the format
        /// `file:///<path>#subdirectory=<subdirectory>`.
        url: VerbatimUrl,
    },
}

impl RequirementSource {
    /// Construct a [`RequirementSource`] for a URL source, given a URL parsed into components and
    /// the PEP 508 string (after the `@`) as [`VerbatimUrl`].
    pub fn from_parsed_url(parsed_url: ParsedUrl, url: VerbatimUrl) -> Self {
        match parsed_url {
            ParsedUrl::Path(local_file) => RequirementSource::Path {
                path: local_file.path,
                url,
                editable: local_file.editable,
            },
            ParsedUrl::Git(git) => RequirementSource::Git {
                url,
                repository: git.url.repository().clone(),
                reference: git.url.reference().clone(),
                precise: git.url.precise(),
                subdirectory: git.subdirectory,
            },
            ParsedUrl::Archive(archive) => RequirementSource::Url {
                url,
                location: archive.url,
                subdirectory: archive.subdirectory,
            },
        }
    }

    /// Returns `true` if the source is editable.
    pub fn is_editable(&self) -> bool {
        matches!(self, Self::Path { editable: true, .. })
    }
}
