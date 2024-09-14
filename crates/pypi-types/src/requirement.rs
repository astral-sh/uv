use std::fmt::{Display, Formatter};
use std::io;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use distribution_filename::DistExtension;
use thiserror::Error;
use url::Url;

use pep440_rs::VersionSpecifiers;
use pep508_rs::{
    marker, MarkerEnvironment, MarkerTree, RequirementOrigin, VerbatimUrl, VersionOrUrl,
};
use uv_fs::{relative_to, PortablePathBuf, CWD};
use uv_git::{GitReference, GitSha, GitUrl};
use uv_normalize::{ExtraName, PackageName};

use crate::{
    Hashes, ParsedArchiveUrl, ParsedDirectoryUrl, ParsedGitUrl, ParsedPathUrl, ParsedUrl,
    ParsedUrlError, VerbatimParsedUrl,
};

#[derive(Debug, Error)]
pub enum RequirementError {
    #[error(transparent)]
    VerbatimUrlError(#[from] pep508_rs::VerbatimUrlError),
    #[error(transparent)]
    ParsedUrlError(#[from] ParsedUrlError),
    #[error(transparent)]
    UrlParseError(#[from] url::ParseError),
    #[error(transparent)]
    OidParseError(#[from] uv_git::OidParseError),
}

/// A representation of dependency on a package, an extension over a PEP 508's requirement.
///
/// The main change is using [`RequirementSource`] to represent all supported package sources over
/// [`VersionOrUrl`], which collapses all URL sources into a single stringly type.
#[derive(
    Hash, Debug, Clone, Eq, PartialEq, Ord, PartialOrd, serde::Serialize, serde::Deserialize,
)]
pub struct Requirement {
    pub name: PackageName,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub extras: Vec<ExtraName>,
    #[serde(
        skip_serializing_if = "marker::ser::is_empty",
        serialize_with = "marker::ser::serialize",
        default
    )]
    pub marker: MarkerTree,
    #[serde(flatten)]
    pub source: RequirementSource,
    #[serde(skip)]
    pub origin: Option<RequirementOrigin>,
}

impl Requirement {
    /// Returns whether the markers apply for the given environment.
    ///
    /// When `env` is `None`, this specifically evaluates all marker
    /// expressions based on the environment to `true`. That is, this provides
    /// environment independent marker evaluation.
    pub fn evaluate_markers(&self, env: Option<&MarkerEnvironment>, extras: &[ExtraName]) -> bool {
        self.marker.evaluate_optional_environment(env, extras)
    }

    /// Returns `true` if the requirement is editable.
    pub fn is_editable(&self) -> bool {
        self.source.is_editable()
    }

    /// Remove any sensitive credentials from the requirement.
    #[must_use]
    pub fn redact(self) -> Requirement {
        match self.source {
            RequirementSource::Git {
                mut repository,
                reference,
                precise,
                subdirectory,
                url,
            } => {
                // Redact the repository URL, but allow `git@`.
                redact_git_credentials(&mut repository);

                // Redact the PEP 508 URL.
                let mut url = url.to_url();
                redact_git_credentials(&mut url);
                let url = VerbatimUrl::from_url(url);

                Self {
                    name: self.name,
                    extras: self.extras,
                    marker: self.marker,
                    source: RequirementSource::Git {
                        repository,
                        reference,
                        precise,
                        subdirectory,
                        url,
                    },
                    origin: self.origin,
                }
            }
            _ => self,
        }
    }

    /// Convert the requirement to a [`Requirement`] relative to the given path.
    pub fn relative_to(self, path: &Path) -> Result<Self, io::Error> {
        Ok(Self {
            source: self.source.relative_to(path)?,
            ..self
        })
    }

    /// Return the hashes of the requirement, as specified in the URL fragment.
    pub fn hashes(&self) -> Option<Hashes> {
        let RequirementSource::Url { ref url, .. } = self.source else {
            return None;
        };
        let fragment = url.fragment()?;
        Hashes::parse_fragment(fragment).ok()
    }
}

impl From<Requirement> for pep508_rs::Requirement<VerbatimUrl> {
    /// Convert a [`Requirement`] to a [`pep508_rs::Requirement`].
    fn from(requirement: Requirement) -> Self {
        pep508_rs::Requirement {
            name: requirement.name,
            extras: requirement.extras,
            marker: requirement.marker,
            origin: requirement.origin,
            version_or_url: match requirement.source {
                RequirementSource::Registry { specifier, .. } => {
                    Some(VersionOrUrl::VersionSpecifier(specifier))
                }
                RequirementSource::Url { url, .. }
                | RequirementSource::Git { url, .. }
                | RequirementSource::Path { url, .. }
                | RequirementSource::Directory { url, .. } => Some(VersionOrUrl::Url(url)),
            },
        }
    }
}

impl From<Requirement> for pep508_rs::Requirement<VerbatimParsedUrl> {
    /// Convert a [`Requirement`] to a [`pep508_rs::Requirement`].
    fn from(requirement: Requirement) -> Self {
        pep508_rs::Requirement {
            name: requirement.name,
            extras: requirement.extras,
            marker: requirement.marker,
            origin: requirement.origin,
            version_or_url: match requirement.source {
                RequirementSource::Registry { specifier, .. } => {
                    Some(VersionOrUrl::VersionSpecifier(specifier))
                }
                RequirementSource::Url {
                    location,
                    subdirectory,
                    ext,
                    url,
                } => Some(VersionOrUrl::Url(VerbatimParsedUrl {
                    parsed_url: ParsedUrl::Archive(ParsedArchiveUrl {
                        url: location,
                        subdirectory,
                        ext,
                    }),
                    verbatim: url,
                })),
                RequirementSource::Git {
                    repository,
                    reference,
                    precise,
                    subdirectory,
                    url,
                } => {
                    let git_url = if let Some(precise) = precise {
                        GitUrl::from_commit(repository, reference, precise)
                    } else {
                        GitUrl::from_reference(repository, reference)
                    };
                    Some(VersionOrUrl::Url(VerbatimParsedUrl {
                        parsed_url: ParsedUrl::Git(ParsedGitUrl {
                            url: git_url,
                            subdirectory,
                        }),
                        verbatim: url,
                    }))
                }
                RequirementSource::Path {
                    install_path,
                    ext,
                    url,
                } => Some(VersionOrUrl::Url(VerbatimParsedUrl {
                    parsed_url: ParsedUrl::Path(ParsedPathUrl {
                        url: url.to_url(),
                        install_path,
                        ext,
                    }),
                    verbatim: url,
                })),
                RequirementSource::Directory {
                    install_path,
                    editable,
                    r#virtual,
                    url,
                } => Some(VersionOrUrl::Url(VerbatimParsedUrl {
                    parsed_url: ParsedUrl::Directory(ParsedDirectoryUrl {
                        url: url.to_url(),
                        install_path,
                        editable,
                        r#virtual,
                    }),
                    verbatim: url,
                })),
            },
        }
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
            RequirementSource::Directory { url, .. } => {
                write!(f, " @ {url}")?;
            }
        }
        if let Some(marker) = self.marker.contents() {
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
#[derive(
    Hash, Debug, Clone, Eq, PartialEq, Ord, PartialOrd, serde::Serialize, serde::Deserialize,
)]
#[serde(try_from = "RequirementSourceWire", into = "RequirementSourceWire")]
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
        /// The remote location of the archive file, without subdirectory fragment.
        location: Url,
        /// For source distributions, the path to the distribution if it is not in the archive
        /// root.
        subdirectory: Option<PathBuf>,
        /// The file extension, e.g. `tar.gz`, `zip`, etc.
        ext: DistExtension,
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
    /// be a binary distribution (a `.whl` file) or a source distribution archive (a `.zip` or
    /// `.tar.gz` file).
    Path {
        /// The absolute path to the distribution which we use for installing.
        install_path: PathBuf,
        /// The file extension, e.g. `tar.gz`, `zip`, etc.
        ext: DistExtension,
        /// The PEP 508 style URL in the format
        /// `file:///<path>#subdirectory=<subdirectory>`.
        url: VerbatimUrl,
    },
    /// A local source tree (a directory with a pyproject.toml in, or a legacy
    /// source distribution with only a setup.py but non pyproject.toml in it).
    Directory {
        /// The absolute path to the distribution which we use for installing.
        install_path: PathBuf,
        /// For a source tree (a directory), whether to install as an editable.
        editable: bool,
        /// For a source tree (a directory), whether the project should be built and installed.
        r#virtual: bool,
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
                install_path: local_file.install_path.clone(),
                ext: local_file.ext,
                url,
            },
            ParsedUrl::Directory(directory) => RequirementSource::Directory {
                install_path: directory.install_path.clone(),
                editable: directory.editable,
                r#virtual: directory.r#virtual,
                url,
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
                ext: archive.ext,
            },
        }
    }

    /// Construct a [`RequirementSource`] for a URL source, given a URL parsed into components.
    pub fn from_verbatim_parsed_url(parsed_url: ParsedUrl) -> Self {
        let verbatim_url = VerbatimUrl::from_url(Url::from(parsed_url.clone()));
        Self::from_parsed_url(parsed_url, verbatim_url)
    }

    /// Convert the source to a [`VerbatimParsedUrl`], if it's a URL source.
    pub fn to_verbatim_parsed_url(&self) -> Option<VerbatimParsedUrl> {
        match &self {
            Self::Registry { .. } => None,
            Self::Url {
                location,
                subdirectory,
                ext,
                url,
            } => Some(VerbatimParsedUrl {
                parsed_url: ParsedUrl::Archive(ParsedArchiveUrl::from_source(
                    location.clone(),
                    subdirectory.clone(),
                    *ext,
                )),
                verbatim: url.clone(),
            }),
            Self::Path {
                install_path,
                ext,
                url,
            } => Some(VerbatimParsedUrl {
                parsed_url: ParsedUrl::Path(ParsedPathUrl::from_source(
                    install_path.clone(),
                    *ext,
                    url.to_url(),
                )),
                verbatim: url.clone(),
            }),
            Self::Directory {
                install_path,
                editable,
                r#virtual,
                url,
            } => Some(VerbatimParsedUrl {
                parsed_url: ParsedUrl::Directory(ParsedDirectoryUrl::from_source(
                    install_path.clone(),
                    *editable,
                    *r#virtual,
                    url.to_url(),
                )),
                verbatim: url.clone(),
            }),
            Self::Git {
                repository,
                reference,
                precise,
                subdirectory,
                url,
            } => Some(VerbatimParsedUrl {
                parsed_url: ParsedUrl::Git(ParsedGitUrl::from_source(
                    repository.clone(),
                    reference.clone(),
                    *precise,
                    subdirectory.clone(),
                )),
                verbatim: url.clone(),
            }),
        }
    }

    /// Convert the source to a version specifier or URL.
    ///
    /// If the source is a registry and the specifier is empty, it returns `None`.
    pub fn version_or_url(&self) -> Option<VersionOrUrl<VerbatimParsedUrl>> {
        match self {
            Self::Registry { specifier, .. } => {
                if specifier.is_empty() {
                    None
                } else {
                    Some(VersionOrUrl::VersionSpecifier(specifier.clone()))
                }
            }
            Self::Url { .. } | Self::Git { .. } | Self::Path { .. } | Self::Directory { .. } => {
                Some(VersionOrUrl::Url(self.to_verbatim_parsed_url()?))
            }
        }
    }

    /// Returns `true` if the source is editable.
    pub fn is_editable(&self) -> bool {
        matches!(self, Self::Directory { editable: true, .. })
    }

    /// If the source is the registry, return the version specifiers
    pub fn version_specifiers(&self) -> Option<&VersionSpecifiers> {
        match self {
            RequirementSource::Registry { specifier, .. } => Some(specifier),
            RequirementSource::Url { .. }
            | RequirementSource::Git { .. }
            | RequirementSource::Path { .. }
            | RequirementSource::Directory { .. } => None,
        }
    }

    /// Convert the source to a [`RequirementSource`] relative to the given path.
    pub fn relative_to(self, path: &Path) -> Result<Self, io::Error> {
        match self {
            RequirementSource::Registry { .. }
            | RequirementSource::Url { .. }
            | RequirementSource::Git { .. } => Ok(self),
            RequirementSource::Path {
                install_path,
                ext,
                url,
            } => Ok(Self::Path {
                install_path: relative_to(&install_path, path)
                    .or_else(|_| std::path::absolute(install_path))?,
                ext,
                url,
            }),
            RequirementSource::Directory {
                install_path,
                editable,
                r#virtual,
                url,
                ..
            } => Ok(Self::Directory {
                install_path: relative_to(&install_path, path)
                    .or_else(|_| std::path::absolute(install_path))?,
                editable,
                r#virtual,
                url,
            }),
        }
    }
}

impl Display for RequirementSource {
    /// Display the [`RequirementSource`], with the intention of being shown directly to a user,
    /// rather than for inclusion in a `requirements.txt` file.
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Registry { specifier, index } => {
                write!(f, "{specifier}")?;
                if let Some(index) = index {
                    write!(f, " (index: {index})")?;
                }
            }
            Self::Url { url, .. } => {
                write!(f, " {url}")?;
            }
            Self::Git {
                url: _,
                repository,
                reference,
                precise: _,
                subdirectory,
            } => {
                write!(f, " git+{repository}")?;
                if let Some(reference) = reference.as_str() {
                    write!(f, "@{reference}")?;
                }
                if let Some(subdirectory) = subdirectory {
                    writeln!(f, "#subdirectory={}", subdirectory.display())?;
                }
            }
            Self::Path { url, .. } => {
                write!(f, "{url}")?;
            }
            Self::Directory { url, .. } => {
                write!(f, "{url}")?;
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
enum RequirementSourceWire {
    /// Ex) `source = { git = "<https://github.com/astral-test/uv-public-pypackage?rev=0.0.1#0dacfd662c64cb4ceb16e6cf65a157a8b715b979>" }`
    Git { git: String },
    /// Ex) `source = { url = "<https://example.org/foo-1.0.zip>" }`
    Direct {
        url: Url,
        subdirectory: Option<String>,
    },
    /// Ex) `source = { path = "/home/ferris/iniconfig-2.0.0-py3-none-any.whl" }`
    Path { path: PortablePathBuf },
    /// Ex) `source = { directory = "/home/ferris/iniconfig" }`
    Directory { directory: PortablePathBuf },
    /// Ex) `source = { editable = "/home/ferris/iniconfig" }`
    Editable { editable: PortablePathBuf },
    /// Ex) `source = { editable = "/home/ferris/iniconfig" }`
    Virtual { r#virtual: PortablePathBuf },
    /// Ex) `source = { specifier = "foo >1,<2" }`
    Registry {
        #[serde(skip_serializing_if = "VersionSpecifiers::is_empty", default)]
        specifier: VersionSpecifiers,
        index: Option<String>,
    },
}

impl From<RequirementSource> for RequirementSourceWire {
    fn from(value: RequirementSource) -> Self {
        match value {
            RequirementSource::Registry { specifier, index } => Self::Registry { specifier, index },
            RequirementSource::Url {
                subdirectory,
                location,
                ext: _,
                url: _,
            } => Self::Direct {
                url: location,
                subdirectory: subdirectory
                    .as_deref()
                    .and_then(Path::to_str)
                    .map(str::to_string),
            },
            RequirementSource::Git {
                repository,
                reference,
                precise,
                subdirectory,
                url: _,
            } => {
                let mut url = repository;

                // Redact the credentials.
                redact_git_credentials(&mut url);

                // Clear out any existing state.
                url.set_fragment(None);
                url.set_query(None);

                // Put the subdirectory in the query.
                if let Some(subdirectory) = subdirectory.as_deref().and_then(Path::to_str) {
                    url.query_pairs_mut()
                        .append_pair("subdirectory", subdirectory);
                }

                // Put the requested reference in the query.
                match reference {
                    GitReference::Branch(branch) => {
                        url.query_pairs_mut()
                            .append_pair("branch", branch.to_string().as_str());
                    }
                    GitReference::Tag(tag) => {
                        url.query_pairs_mut()
                            .append_pair("tag", tag.to_string().as_str());
                    }
                    GitReference::ShortCommit(rev)
                    | GitReference::BranchOrTag(rev)
                    | GitReference::BranchOrTagOrCommit(rev)
                    | GitReference::NamedRef(rev)
                    | GitReference::FullCommit(rev) => {
                        url.query_pairs_mut()
                            .append_pair("rev", rev.to_string().as_str());
                    }
                    GitReference::DefaultBranch => {}
                }

                // Put the precise commit in the fragment.
                if let Some(precise) = precise {
                    url.set_fragment(Some(&precise.to_string()));
                }

                Self::Git {
                    git: url.to_string(),
                }
            }
            RequirementSource::Path {
                install_path,
                ext: _,
                url: _,
            } => Self::Path {
                path: PortablePathBuf::from(install_path),
            },
            RequirementSource::Directory {
                install_path,
                editable,
                r#virtual,
                url: _,
            } => {
                if editable {
                    Self::Editable {
                        editable: PortablePathBuf::from(install_path),
                    }
                } else if r#virtual {
                    Self::Virtual {
                        r#virtual: PortablePathBuf::from(install_path),
                    }
                } else {
                    Self::Directory {
                        directory: PortablePathBuf::from(install_path),
                    }
                }
            }
        }
    }
}

impl TryFrom<RequirementSourceWire> for RequirementSource {
    type Error = RequirementError;

    fn try_from(wire: RequirementSourceWire) -> Result<RequirementSource, RequirementError> {
        match wire {
            RequirementSourceWire::Registry { specifier, index } => {
                Ok(Self::Registry { specifier, index })
            }
            RequirementSourceWire::Git { git } => {
                let mut repository = Url::parse(&git)?;

                let mut reference = GitReference::DefaultBranch;
                let mut subdirectory = None;
                for (key, val) in repository.query_pairs() {
                    match &*key {
                        "tag" => reference = GitReference::Tag(val.into_owned()),
                        "branch" => reference = GitReference::Branch(val.into_owned()),
                        "rev" => reference = GitReference::from_rev(val.into_owned()),
                        "subdirectory" => subdirectory = Some(val.into_owned()),
                        _ => continue,
                    };
                }

                let precise = repository.fragment().map(GitSha::from_str).transpose()?;

                // Clear out any existing state.
                repository.set_fragment(None);
                repository.set_query(None);

                // Redact the credentials.
                redact_git_credentials(&mut repository);

                // Create a PEP 508-compatible URL.
                let mut url = Url::parse(&format!("git+{repository}"))?;
                if let Some(rev) = reference.as_str() {
                    url.set_path(&format!("{}@{}", url.path(), rev));
                }
                if let Some(subdirectory) = &subdirectory {
                    url.set_fragment(Some(&format!("subdirectory={subdirectory}")));
                }
                let url = VerbatimUrl::from_url(url);

                Ok(Self::Git {
                    repository,
                    reference,
                    precise,
                    subdirectory: subdirectory.map(PathBuf::from),
                    url,
                })
            }
            RequirementSourceWire::Direct { url, subdirectory } => Ok(Self::Url {
                url: VerbatimUrl::from_url(url.clone()),
                location: url.clone(),
                subdirectory: subdirectory.map(PathBuf::from),
                ext: DistExtension::from_path(url.path())
                    .map_err(|err| ParsedUrlError::MissingExtensionUrl(url.to_string(), err))?,
            }),
            // TODO(charlie): The use of `CWD` here is incorrect. These should be resolved relative
            // to the workspace root, but we don't have access to it here. When comparing these
            // sources in the lockfile, we replace the URL anyway.
            RequirementSourceWire::Path { path } => {
                let path = PathBuf::from(path);
                let url = VerbatimUrl::from_path(&path, &*CWD)?;
                Ok(Self::Path {
                    ext: DistExtension::from_path(path.as_path())
                        .map_err(|err| ParsedUrlError::MissingExtensionPath(path.clone(), err))?,
                    install_path: path,
                    url,
                })
            }
            RequirementSourceWire::Directory { directory } => {
                let directory = PathBuf::from(directory);
                let url = VerbatimUrl::from_path(&directory, &*CWD)?;
                Ok(Self::Directory {
                    install_path: directory,
                    editable: false,
                    r#virtual: false,
                    url,
                })
            }
            RequirementSourceWire::Editable { editable } => {
                let editable = PathBuf::from(editable);
                let url = VerbatimUrl::from_path(&editable, &*CWD)?;
                Ok(Self::Directory {
                    install_path: editable,
                    editable: true,
                    r#virtual: false,
                    url,
                })
            }
            RequirementSourceWire::Virtual { r#virtual } => {
                let r#virtual = PathBuf::from(r#virtual);
                let url = VerbatimUrl::from_path(&r#virtual, &*CWD)?;
                Ok(Self::Directory {
                    install_path: r#virtual,
                    editable: false,
                    r#virtual: true,
                    url,
                })
            }
        }
    }
}

/// Remove the credentials from a Git URL, allowing the generic `git` username (without a password)
/// in SSH URLs, as in, `ssh://git@github.com/...`.
pub fn redact_git_credentials(url: &mut Url) {
    // For URLs that use the `git` convention (i.e., `ssh://git@github.com/...`), avoid dropping the
    // username.
    if url.scheme() == "ssh" && url.username() == "git" && url.password().is_none() {
        return;
    }
    let _ = url.set_password(None);
    let _ = url.set_username("");
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use pep508_rs::{MarkerTree, VerbatimUrl};

    use crate::{Requirement, RequirementSource};

    #[test]
    fn roundtrip() {
        let requirement = Requirement {
            name: "foo".parse().unwrap(),
            extras: vec![],
            marker: MarkerTree::TRUE,
            source: RequirementSource::Registry {
                specifier: ">1,<2".parse().unwrap(),
                index: None,
            },
            origin: None,
        };

        let raw = toml::to_string(&requirement).unwrap();
        let deserialized: Requirement = toml::from_str(&raw).unwrap();
        assert_eq!(requirement, deserialized);

        let path = if cfg!(windows) {
            "C:\\home\\ferris\\foo"
        } else {
            "/home/ferris/foo"
        };
        let requirement = Requirement {
            name: "foo".parse().unwrap(),
            extras: vec![],
            marker: MarkerTree::TRUE,
            source: RequirementSource::Directory {
                install_path: PathBuf::from(path),
                editable: false,
                r#virtual: false,
                url: VerbatimUrl::from_absolute_path(path).unwrap(),
            },
            origin: None,
        };

        let raw = toml::to_string(&requirement).unwrap();
        let deserialized: Requirement = toml::from_str(&raw).unwrap();
        assert_eq!(requirement, deserialized);
    }
}
