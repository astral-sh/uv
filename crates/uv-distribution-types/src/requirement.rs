use std::fmt::{Display, Formatter};
use std::io;
use std::path::Path;
use std::str::FromStr;

use thiserror::Error;
use uv_cache_key::{CacheKey, CacheKeyHasher};
use uv_distribution_filename::DistExtension;
use uv_fs::{CWD, PortablePath, PortablePathBuf, relative_to};
use uv_git_types::{GitOid, GitReference, GitUrl, GitUrlParseError, OidParseError};
use uv_normalize::{ExtraName, GroupName, PackageName};
use uv_pep440::VersionSpecifiers;
use uv_pep508::{
    MarkerEnvironment, MarkerTree, RequirementOrigin, VerbatimUrl, VersionOrUrl, marker,
};
use uv_redacted::DisplaySafeUrl;

use crate::{IndexMetadata, IndexUrl};

use uv_pypi_types::{
    ConflictItem, Hashes, ParsedArchiveUrl, ParsedDirectoryUrl, ParsedGitUrl, ParsedPathUrl,
    ParsedUrl, ParsedUrlError, VerbatimParsedUrl,
};

#[derive(Debug, Error)]
pub enum RequirementError {
    #[error(transparent)]
    VerbatimUrlError(#[from] uv_pep508::VerbatimUrlError),
    #[error(transparent)]
    ParsedUrlError(#[from] ParsedUrlError),
    #[error(transparent)]
    UrlParseError(#[from] url::ParseError),
    #[error(transparent)]
    OidParseError(#[from] OidParseError),
    #[error(transparent)]
    GitUrlParse(#[from] GitUrlParseError),
}

/// A representation of dependency on a package, an extension over a PEP 508's requirement.
///
/// The main change is using [`RequirementSource`] to represent all supported package sources over
/// [`VersionOrUrl`], which collapses all URL sources into a single stringly type.
///
/// Additionally, this requirement type makes room for dependency groups, which lack a standardized
/// representation in PEP 508. In the context of this type, extras and groups are assumed to be
/// mutually exclusive, in that if `extras` is non-empty, `groups` must be empty and vice versa.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Requirement {
    pub name: PackageName,
    #[serde(skip_serializing_if = "<[ExtraName]>::is_empty", default)]
    pub extras: Box<[ExtraName]>,
    #[serde(skip_serializing_if = "<[GroupName]>::is_empty", default)]
    pub groups: Box<[GroupName]>,
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

    /// Convert to a [`Requirement`] with a relative path based on the given root.
    pub fn relative_to(self, path: &Path) -> Result<Self, io::Error> {
        Ok(Self {
            source: self.source.relative_to(path)?,
            ..self
        })
    }

    /// Convert to a [`Requirement`] with an absolute path based on the given root.
    #[must_use]
    pub fn to_absolute(self, path: &Path) -> Self {
        Self {
            source: self.source.to_absolute(path),
            ..self
        }
    }

    /// Return the hashes of the requirement, as specified in the URL fragment.
    pub fn hashes(&self) -> Option<Hashes> {
        let RequirementSource::Url { ref url, .. } = self.source else {
            return None;
        };
        let fragment = url.fragment()?;
        Hashes::parse_fragment(fragment).ok()
    }

    /// Set the source file containing the requirement.
    #[must_use]
    pub fn with_origin(self, origin: RequirementOrigin) -> Self {
        Self {
            origin: Some(origin),
            ..self
        }
    }
}

impl std::hash::Hash for Requirement {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        let Self {
            name,
            extras,
            groups,
            marker,
            source,
            origin: _,
        } = self;
        name.hash(state);
        extras.hash(state);
        groups.hash(state);
        marker.hash(state);
        source.hash(state);
    }
}

impl PartialEq for Requirement {
    fn eq(&self, other: &Self) -> bool {
        let Self {
            name,
            extras,
            groups,
            marker,
            source,
            origin: _,
        } = self;
        let Self {
            name: other_name,
            extras: other_extras,
            groups: other_groups,
            marker: other_marker,
            source: other_source,
            origin: _,
        } = other;
        name == other_name
            && extras == other_extras
            && groups == other_groups
            && marker == other_marker
            && source == other_source
    }
}

impl Eq for Requirement {}

impl Ord for Requirement {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let Self {
            name,
            extras,
            groups,
            marker,
            source,
            origin: _,
        } = self;
        let Self {
            name: other_name,
            extras: other_extras,
            groups: other_groups,
            marker: other_marker,
            source: other_source,
            origin: _,
        } = other;
        name.cmp(other_name)
            .then_with(|| extras.cmp(other_extras))
            .then_with(|| groups.cmp(other_groups))
            .then_with(|| marker.cmp(other_marker))
            .then_with(|| source.cmp(other_source))
    }
}

impl PartialOrd for Requirement {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl From<Requirement> for uv_pep508::Requirement<VerbatimUrl> {
    /// Convert a [`Requirement`] to a [`uv_pep508::Requirement`].
    fn from(requirement: Requirement) -> Self {
        Self {
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

impl From<Requirement> for uv_pep508::Requirement<VerbatimParsedUrl> {
    /// Convert a [`Requirement`] to a [`uv_pep508::Requirement`].
    fn from(requirement: Requirement) -> Self {
        Self {
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
                    git,
                    subdirectory,
                    url,
                } => Some(VersionOrUrl::Url(VerbatimParsedUrl {
                    parsed_url: ParsedUrl::Git(ParsedGitUrl {
                        url: git,
                        subdirectory,
                    }),
                    verbatim: url,
                })),
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

impl From<uv_pep508::Requirement<VerbatimParsedUrl>> for Requirement {
    /// Convert a [`uv_pep508::Requirement`] to a [`Requirement`].
    fn from(requirement: uv_pep508::Requirement<VerbatimParsedUrl>) -> Self {
        let source = match requirement.version_or_url {
            None => RequirementSource::Registry {
                specifier: VersionSpecifiers::empty(),
                index: None,
                conflict: None,
            },
            // The most popular case: just a name, a version range and maybe extras.
            Some(VersionOrUrl::VersionSpecifier(specifier)) => RequirementSource::Registry {
                specifier,
                index: None,
                conflict: None,
            },
            Some(VersionOrUrl::Url(url)) => {
                RequirementSource::from_parsed_url(url.parsed_url, url.verbatim)
            }
        };
        Self {
            name: requirement.name,
            groups: Box::new([]),
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
            RequirementSource::Registry {
                specifier, index, ..
            } => {
                write!(f, "{specifier}")?;
                if let Some(index) = index {
                    write!(f, " (index: {})", index.url)?;
                }
            }
            RequirementSource::Url { url, .. } => {
                write!(f, " @ {url}")?;
            }
            RequirementSource::Git {
                url: _,
                git,
                subdirectory,
            } => {
                write!(f, " @ git+{}", git.repository())?;
                if let Some(reference) = git.reference().as_str() {
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

impl CacheKey for Requirement {
    fn cache_key(&self, state: &mut CacheKeyHasher) {
        self.name.as_str().cache_key(state);

        self.groups.len().cache_key(state);
        for group in &self.groups {
            group.as_str().cache_key(state);
        }

        self.extras.len().cache_key(state);
        for extra in &self.extras {
            extra.as_str().cache_key(state);
        }

        if let Some(marker) = self.marker.contents() {
            1u8.cache_key(state);
            marker.to_string().cache_key(state);
        } else {
            0u8.cache_key(state);
        }

        match &self.source {
            RequirementSource::Registry {
                specifier,
                index,
                conflict: _,
            } => {
                0u8.cache_key(state);
                specifier.len().cache_key(state);
                for spec in specifier.iter() {
                    spec.operator().as_str().cache_key(state);
                    spec.version().cache_key(state);
                }
                if let Some(index) = index {
                    1u8.cache_key(state);
                    index.url.cache_key(state);
                } else {
                    0u8.cache_key(state);
                }
                // `conflict` is intentionally omitted
            }
            RequirementSource::Url {
                location,
                subdirectory,
                ext,
                url,
            } => {
                1u8.cache_key(state);
                location.cache_key(state);
                if let Some(subdirectory) = subdirectory {
                    1u8.cache_key(state);
                    subdirectory.display().to_string().cache_key(state);
                } else {
                    0u8.cache_key(state);
                }
                ext.name().cache_key(state);
                url.cache_key(state);
            }
            RequirementSource::Git {
                git,
                subdirectory,
                url,
            } => {
                2u8.cache_key(state);
                git.to_string().cache_key(state);
                if let Some(subdirectory) = subdirectory {
                    1u8.cache_key(state);
                    subdirectory.display().to_string().cache_key(state);
                } else {
                    0u8.cache_key(state);
                }
                url.cache_key(state);
            }
            RequirementSource::Path {
                install_path,
                ext,
                url,
            } => {
                3u8.cache_key(state);
                install_path.cache_key(state);
                ext.name().cache_key(state);
                url.cache_key(state);
            }
            RequirementSource::Directory {
                install_path,
                editable,
                r#virtual,
                url,
            } => {
                4u8.cache_key(state);
                install_path.cache_key(state);
                editable.cache_key(state);
                r#virtual.cache_key(state);
                url.cache_key(state);
            }
        }

        // `origin` is intentionally omitted
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
        /// Choose a version from the index at the given URL.
        index: Option<IndexMetadata>,
        /// The conflict item associated with the source, if any.
        conflict: Option<ConflictItem>,
    },
    // TODO(konsti): Track and verify version specifier from `project.dependencies` matches the
    // version in remote location.
    /// A remote `http://` or `https://` URL, either a built distribution,
    /// e.g. `foo @ https://example.org/foo-1.0-py3-none-any.whl`, or a source distribution,
    /// e.g.`foo @ https://example.org/foo-1.0.zip`.
    Url {
        /// The remote location of the archive file, without subdirectory fragment.
        location: DisplaySafeUrl,
        /// For source distributions, the path to the distribution if it is not in the archive
        /// root.
        subdirectory: Option<Box<Path>>,
        /// The file extension, e.g. `tar.gz`, `zip`, etc.
        ext: DistExtension,
        /// The PEP 508 style URL in the format
        /// `<scheme>://<domain>/<path>#subdirectory=<subdirectory>`.
        url: VerbatimUrl,
    },
    /// A remote Git repository, over either HTTPS or SSH.
    Git {
        /// The repository URL and reference to the commit to use.
        git: GitUrl,
        /// The path to the source distribution if it is not in the repository root.
        subdirectory: Option<Box<Path>>,
        /// The PEP 508 style url in the format
        /// `git+<scheme>://<domain>/<path>@<rev>#subdirectory=<subdirectory>`.
        url: VerbatimUrl,
    },
    /// A local built or source distribution, either from a path or a `file://` URL. It can either
    /// be a binary distribution (a `.whl` file) or a source distribution archive (a `.zip` or
    /// `.tar.gz` file).
    Path {
        /// The absolute path to the distribution which we use for installing.
        install_path: Box<Path>,
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
        install_path: Box<Path>,
        /// For a source tree (a directory), whether to install as an editable.
        editable: Option<bool>,
        /// For a source tree (a directory), whether the project should be built and installed.
        r#virtual: Option<bool>,
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
            ParsedUrl::Path(local_file) => Self::Path {
                install_path: local_file.install_path.clone(),
                ext: local_file.ext,
                url,
            },
            ParsedUrl::Directory(directory) => Self::Directory {
                install_path: directory.install_path.clone(),
                editable: directory.editable,
                r#virtual: directory.r#virtual,
                url,
            },
            ParsedUrl::Git(git) => Self::Git {
                git: git.url.clone(),
                url,
                subdirectory: git.subdirectory,
            },
            ParsedUrl::Archive(archive) => Self::Url {
                url,
                location: archive.url,
                subdirectory: archive.subdirectory,
                ext: archive.ext,
            },
        }
    }

    /// Convert the source to a [`VerbatimParsedUrl`], if it's a URL source.
    pub fn to_verbatim_parsed_url(&self) -> Option<VerbatimParsedUrl> {
        match self {
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
                git,
                subdirectory,
                url,
            } => Some(VerbatimParsedUrl {
                parsed_url: ParsedUrl::Git(ParsedGitUrl::from_source(
                    git.clone(),
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
        matches!(
            self,
            Self::Directory {
                editable: Some(true),
                ..
            }
        )
    }

    /// Returns `true` if the source is empty.
    pub fn is_empty(&self) -> bool {
        match self {
            Self::Registry { specifier, .. } => specifier.is_empty(),
            Self::Url { .. } | Self::Git { .. } | Self::Path { .. } | Self::Directory { .. } => {
                false
            }
        }
    }

    /// If the source is the registry, return the version specifiers
    pub fn version_specifiers(&self) -> Option<&VersionSpecifiers> {
        match self {
            Self::Registry { specifier, .. } => Some(specifier),
            Self::Url { .. } | Self::Git { .. } | Self::Path { .. } | Self::Directory { .. } => {
                None
            }
        }
    }

    /// Convert the source to a [`RequirementSource`] relative to the given path.
    pub fn relative_to(self, path: &Path) -> Result<Self, io::Error> {
        match self {
            Self::Registry { .. } | Self::Url { .. } | Self::Git { .. } => Ok(self),
            Self::Path {
                install_path,
                ext,
                url,
            } => Ok(Self::Path {
                install_path: relative_to(&install_path, path)
                    .or_else(|_| std::path::absolute(install_path))?
                    .into_boxed_path(),
                ext,
                url,
            }),
            Self::Directory {
                install_path,
                editable,
                r#virtual,
                url,
                ..
            } => Ok(Self::Directory {
                install_path: relative_to(&install_path, path)
                    .or_else(|_| std::path::absolute(install_path))?
                    .into_boxed_path(),
                editable,
                r#virtual,
                url,
            }),
        }
    }

    /// Convert the source to a [`RequirementSource`] with an absolute path based on the given root.
    #[must_use]
    pub fn to_absolute(self, root: &Path) -> Self {
        match self {
            Self::Registry { .. } | Self::Url { .. } | Self::Git { .. } => self,
            Self::Path {
                install_path,
                ext,
                url,
            } => Self::Path {
                install_path: uv_fs::normalize_path_buf(root.join(install_path)).into_boxed_path(),
                ext,
                url,
            },
            Self::Directory {
                install_path,
                editable,
                r#virtual,
                url,
                ..
            } => Self::Directory {
                install_path: uv_fs::normalize_path_buf(root.join(install_path)).into_boxed_path(),
                editable,
                r#virtual,
                url,
            },
        }
    }
}

impl Display for RequirementSource {
    /// Display the [`RequirementSource`], with the intention of being shown directly to a user,
    /// rather than for inclusion in a `requirements.txt` file.
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Registry {
                specifier, index, ..
            } => {
                write!(f, "{specifier}")?;
                if let Some(index) = index {
                    write!(f, " (index: {})", index.url)?;
                }
            }
            Self::Url { url, .. } => {
                write!(f, " {url}")?;
            }
            Self::Git {
                url: _,
                git,
                subdirectory,
            } => {
                write!(f, " git+{}", git.repository())?;
                if let Some(reference) = git.reference().as_str() {
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
        url: DisplaySafeUrl,
        subdirectory: Option<PortablePathBuf>,
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
        index: Option<DisplaySafeUrl>,
        conflict: Option<ConflictItem>,
    },
}

impl From<RequirementSource> for RequirementSourceWire {
    fn from(value: RequirementSource) -> Self {
        match value {
            RequirementSource::Registry {
                specifier,
                index,
                conflict,
            } => {
                let index = index.map(|index| index.url.into_url()).map(|mut index| {
                    index.remove_credentials();
                    index
                });
                Self::Registry {
                    specifier,
                    index,
                    conflict,
                }
            }
            RequirementSource::Url {
                subdirectory,
                location,
                ext: _,
                url: _,
            } => Self::Direct {
                url: location,
                subdirectory: subdirectory.map(PortablePathBuf::from),
            },
            RequirementSource::Git {
                git,
                subdirectory,
                url: _,
            } => {
                let mut url = git.repository().clone();

                // Remove the credentials.
                url.remove_credentials();

                // Clear out any existing state.
                url.set_fragment(None);
                url.set_query(None);

                // Put the subdirectory in the query.
                if let Some(subdirectory) = subdirectory
                    .as_deref()
                    .map(PortablePath::from)
                    .as_ref()
                    .map(PortablePath::to_string)
                {
                    url.query_pairs_mut()
                        .append_pair("subdirectory", &subdirectory);
                }

                // Put the requested reference in the query.
                match git.reference() {
                    GitReference::Branch(branch) => {
                        url.query_pairs_mut().append_pair("branch", branch.as_str());
                    }
                    GitReference::Tag(tag) => {
                        url.query_pairs_mut().append_pair("tag", tag.as_str());
                    }
                    GitReference::BranchOrTag(rev)
                    | GitReference::BranchOrTagOrCommit(rev)
                    | GitReference::NamedRef(rev) => {
                        url.query_pairs_mut().append_pair("rev", rev.as_str());
                    }
                    GitReference::DefaultBranch => {}
                }

                // Put the precise commit in the fragment.
                if let Some(precise) = git.precise() {
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
                if editable.unwrap_or(false) {
                    Self::Editable {
                        editable: PortablePathBuf::from(install_path),
                    }
                } else if r#virtual.unwrap_or(false) {
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

    fn try_from(wire: RequirementSourceWire) -> Result<Self, RequirementError> {
        match wire {
            RequirementSourceWire::Registry {
                specifier,
                index,
                conflict,
            } => Ok(Self::Registry {
                specifier,
                index: index
                    .map(|index| IndexMetadata::from(IndexUrl::from(VerbatimUrl::from_url(index)))),
                conflict,
            }),
            RequirementSourceWire::Git { git } => {
                let mut repository = DisplaySafeUrl::parse(&git)?;

                let mut reference = GitReference::DefaultBranch;
                let mut subdirectory: Option<PortablePathBuf> = None;
                for (key, val) in repository.query_pairs() {
                    match &*key {
                        "tag" => reference = GitReference::Tag(val.into_owned()),
                        "branch" => reference = GitReference::Branch(val.into_owned()),
                        "rev" => reference = GitReference::from_rev(val.into_owned()),
                        "subdirectory" => {
                            subdirectory = Some(PortablePathBuf::from(val.as_ref()));
                        }
                        _ => {}
                    }
                }

                let precise = repository.fragment().map(GitOid::from_str).transpose()?;

                // Clear out any existing state.
                repository.set_fragment(None);
                repository.set_query(None);

                // Remove the credentials.
                repository.remove_credentials();

                // Create a PEP 508-compatible URL.
                let mut url = DisplaySafeUrl::parse(&format!("git+{repository}"))?;
                if let Some(rev) = reference.as_str() {
                    let path = format!("{}@{}", url.path(), rev);
                    url.set_path(&path);
                }
                if let Some(subdirectory) = subdirectory.as_ref() {
                    url.set_fragment(Some(&format!("subdirectory={subdirectory}")));
                }
                let url = VerbatimUrl::from_url(url);

                Ok(Self::Git {
                    git: GitUrl::from_fields(repository, reference, precise)?,
                    subdirectory: subdirectory.map(Box::<Path>::from),
                    url,
                })
            }
            RequirementSourceWire::Direct { url, subdirectory } => {
                let location = url.clone();

                // Create a PEP 508-compatible URL.
                let mut url = url.clone();
                if let Some(subdirectory) = &subdirectory {
                    url.set_fragment(Some(&format!("subdirectory={subdirectory}")));
                }

                Ok(Self::Url {
                    location,
                    subdirectory: subdirectory.map(Box::<Path>::from),
                    ext: DistExtension::from_path(url.path())
                        .map_err(|err| ParsedUrlError::MissingExtensionUrl(url.to_string(), err))?,
                    url: VerbatimUrl::from_url(url.clone()),
                })
            }
            // TODO(charlie): The use of `CWD` here is incorrect. These should be resolved relative
            // to the workspace root, but we don't have access to it here. When comparing these
            // sources in the lockfile, we replace the URL anyway. Ideally, we'd either remove the
            // URL field or make it optional.
            RequirementSourceWire::Path { path } => {
                let path = Box::<Path>::from(path);
                let url =
                    VerbatimUrl::from_normalized_path(uv_fs::normalize_path_buf(CWD.join(&path)))?;
                Ok(Self::Path {
                    ext: DistExtension::from_path(&path).map_err(|err| {
                        ParsedUrlError::MissingExtensionPath(path.to_path_buf(), err)
                    })?,
                    install_path: path,
                    url,
                })
            }
            RequirementSourceWire::Directory { directory } => {
                let directory = Box::<Path>::from(directory);
                let url = VerbatimUrl::from_normalized_path(uv_fs::normalize_path_buf(
                    CWD.join(&directory),
                ))?;
                Ok(Self::Directory {
                    install_path: directory,
                    editable: Some(false),
                    r#virtual: Some(false),
                    url,
                })
            }
            RequirementSourceWire::Editable { editable } => {
                let editable = Box::<Path>::from(editable);
                let url = VerbatimUrl::from_normalized_path(uv_fs::normalize_path_buf(
                    CWD.join(&editable),
                ))?;
                Ok(Self::Directory {
                    install_path: editable,
                    editable: Some(true),
                    r#virtual: Some(false),
                    url,
                })
            }
            RequirementSourceWire::Virtual { r#virtual } => {
                let r#virtual = Box::<Path>::from(r#virtual);
                let url = VerbatimUrl::from_normalized_path(uv_fs::normalize_path_buf(
                    CWD.join(&r#virtual),
                ))?;
                Ok(Self::Directory {
                    install_path: r#virtual,
                    editable: Some(false),
                    r#virtual: Some(true),
                    url,
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use uv_pep508::{MarkerTree, VerbatimUrl};

    use crate::{Requirement, RequirementSource};

    #[test]
    fn roundtrip() {
        let requirement = Requirement {
            name: "foo".parse().unwrap(),
            extras: Box::new([]),
            groups: Box::new([]),
            marker: MarkerTree::TRUE,
            source: RequirementSource::Registry {
                specifier: ">1,<2".parse().unwrap(),
                index: None,
                conflict: None,
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
            extras: Box::new([]),
            groups: Box::new([]),
            marker: MarkerTree::TRUE,
            source: RequirementSource::Directory {
                install_path: PathBuf::from(path).into_boxed_path(),
                editable: Some(false),
                r#virtual: Some(false),
                url: VerbatimUrl::from_absolute_path(path).unwrap(),
            },
            origin: None,
        };

        let raw = toml::to_string(&requirement).unwrap();
        let deserialized: Requirement = toml::from_str(&raw).unwrap();
        assert_eq!(requirement, deserialized);
    }
}
