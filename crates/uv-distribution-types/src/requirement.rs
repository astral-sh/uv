use std::fmt::{Display, Formatter};
use std::io;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use serde::ser::{Serialize, SerializeStruct, Serializer};
use thiserror::Error;
use uv_cache_key::{CacheKey, CacheKeyHasher};
use uv_distribution_filename::DistExtension;
use uv_fs::{CWD, PortablePath, PortablePathBuf, normalize_path, try_relative_to_if};
use uv_git_types::{GitLfs, GitOid, GitReference, GitUrl, GitUrlParseError, OidParseError};
use uv_normalize::{ExtraName, GroupName, PackageName};
use uv_pep440::VersionSpecifiers;
use uv_pep508::{
    MarkerEnvironment, MarkerTree, RequirementOrigin, VerbatimUrl, VersionOrUrl, marker,
};
use uv_redacted::policy::{Raw, Stripped};
use uv_redacted::{SafeUrl, SafeUrlError};

use crate::{IndexMetadata, IndexUrl};

use uv_pypi_types::{
    ConflictItem, HashError, Hashes, ParsedArchiveUrl, ParsedDirectoryUrl, ParsedGitDirectoryUrl,
    ParsedGitPathUrl, ParsedPathUrl, ParsedUrl, ParsedUrlError, VerbatimParsedUrl,
};

#[derive(Debug, Error)]
enum RequirementError {
    #[error(transparent)]
    VerbatimUrlError(#[from] uv_pep508::VerbatimUrlError),
    #[error(transparent)]
    ParsedUrlError(#[from] ParsedUrlError),
    #[error(transparent)]
    UrlParseError(#[from] SafeUrlError),
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
    pub scope: RequirementScope,
    #[serde(skip)]
    pub origin: Option<RequirementOrigin>,
}

/// Whether a requirement is global or belongs to a package's dependency group.
#[derive(Debug, Clone, Default, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub enum RequirementScope {
    #[default]
    Global,
    Group {
        package: PackageName,
        group: GroupName,
    },
}

impl RequirementScope {
    /// Return the conflict item whose activation makes this requirement visible.
    pub fn conflict_item(&self) -> Option<ConflictItem> {
        match self {
            Self::Global => None,
            Self::Group { package, group } => {
                Some(ConflictItem::from((package.clone(), group.clone())))
            }
        }
    }
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

    /// Convert to a [`Requirement`] with a relative path based on the given root.
    pub fn relative_to(self, path: &Path) -> Result<Self, io::Error> {
        Ok(Self {
            source: self.source.relative_to(path)?,
            ..self
        })
    }

    /// Convert to a [`Requirement`] with an absolute path based on the given root.
    #[must_use]
    pub(crate) fn into_absolute(self, path: &Path) -> Self {
        Self {
            source: self.source.into_absolute(path),
            ..self
        }
    }

    /// Set whether this requirement's local source should be represented by a relative path.
    ///
    /// When `false`, preserve the original input's path preference. Non-local sources are unchanged.
    pub fn set_force_relative(&mut self, force_relative: bool) {
        if let RequirementSource::Path { url, .. } | RequirementSource::Directory { url, .. } =
            &mut self.source
            && url.force_relative() != force_relative
        {
            *url = url.clone().with_force_relative(force_relative);
        }
    }

    /// Return the hashes of the requirement, as specified in the URL fragment.
    pub fn hashes(&self) -> Result<Option<Hashes>, HashError> {
        let (RequirementSource::Url { ref url, .. } | RequirementSource::Path { ref url, .. }) =
            self.source
        else {
            return Ok(None);
        };
        let Some(fragment) = url.fragment() else {
            return Ok(None);
        };
        Hashes::parse_url_fragment(fragment)
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
            scope,
            origin: _,
        } = self;
        name.hash(state);
        extras.hash(state);
        groups.hash(state);
        marker.hash(state);
        source.hash(state);
        scope.hash(state);
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
            scope,
            origin: _,
        } = self;
        let Self {
            name: other_name,
            extras: other_extras,
            groups: other_groups,
            marker: other_marker,
            source: other_source,
            scope: other_scope,
            origin: _,
        } = other;
        name == other_name
            && extras == other_extras
            && groups == other_groups
            && marker == other_marker
            && source == other_source
            && scope == other_scope
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
            scope,
            origin: _,
        } = self;
        let Self {
            name: other_name,
            extras: other_extras,
            groups: other_groups,
            marker: other_marker,
            source: other_source,
            scope: other_scope,
            origin: _,
        } = other;
        name.cmp(other_name)
            .then_with(|| extras.cmp(other_extras))
            .then_with(|| groups.cmp(other_groups))
            .then_with(|| marker.cmp(other_marker))
            .then_with(|| source.cmp(other_source))
            .then_with(|| scope.cmp(other_scope))
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
                | RequirementSource::GitPath { url, .. }
                | RequirementSource::GitDirectory { url, .. }
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
                        url: location.into_url(),
                        subdirectory,
                        ext,
                    }),
                    verbatim: url,
                })),
                RequirementSource::GitDirectory {
                    git,
                    subdirectory,
                    url,
                } => Some(VersionOrUrl::Url(VerbatimParsedUrl {
                    parsed_url: ParsedUrl::GitDirectory(ParsedGitDirectoryUrl {
                        url: git,
                        subdirectory,
                    }),
                    verbatim: url,
                })),
                RequirementSource::GitPath {
                    git,
                    install_path,
                    ext,
                    url,
                } => Some(VersionOrUrl::Url(VerbatimParsedUrl {
                    parsed_url: ParsedUrl::GitPath(ParsedGitPathUrl {
                        url: git,
                        install_path,
                        ext,
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
            scope: RequirementScope::Global,
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
            RequirementSource::GitDirectory {
                url: _,
                git,
                subdirectory,
            } => {
                write!(f, " @ git+{}", git.url())?;
                if let Some(reference) = git.reference().as_url_rev() {
                    write!(f, "@{reference}")?;
                }
                if let Some(subdirectory) = subdirectory {
                    writeln!(f, "#subdirectory={}", subdirectory.display())?;
                }
                if git.lfs().enabled() {
                    writeln!(
                        f,
                        "{}lfs=true",
                        if subdirectory.is_some() { "&" } else { "#" }
                    )?;
                }
            }
            RequirementSource::GitPath {
                url: _,
                git,
                install_path,
                ext: _,
            } => {
                write!(f, " @ git+{}", git.url())?;
                if let Some(reference) = git.reference().as_url_rev() {
                    write!(f, "@{reference}")?;
                }
                write!(f, "#path={}", install_path.display())?;
                if git.lfs().enabled() {
                    write!(f, "&lfs=true")?;
                }
                writeln!(f)?;
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
            RequirementSource::GitDirectory {
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
                if git.lfs().enabled() {
                    1u8.cache_key(state);
                }
                url.cache_key(state);
            }
            RequirementSource::GitPath {
                git,
                install_path,
                ext,
                url,
            } => {
                5u8.cache_key(state);
                git.to_string().cache_key(state);
                install_path.cache_key(state);
                ext.name().cache_key(state);
                if git.lfs().enabled() {
                    1u8.cache_key(state);
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
#[derive(Hash, Debug, Clone, Eq, PartialEq, Ord, PartialOrd, serde::Deserialize)]
#[serde(try_from = "RequirementSourceWire")]
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
        location: SafeUrl<Raw>,
        /// For source distributions, the path to the distribution if it is not in the archive
        /// root.
        subdirectory: Option<Box<Path>>,
        /// The file extension, e.g. `tar.gz`, `zip`, etc.
        ext: DistExtension,
        /// The PEP 508 style URL in the format
        /// `<scheme>://<domain>/<path>#subdirectory=<subdirectory>`.
        url: VerbatimUrl,
    },
    /// A remote Git source tree, over either HTTPS or SSH.
    GitDirectory {
        /// The repository URL and reference to the commit to use.
        git: GitUrl,
        /// The path to the source distribution if it is not in the repository root.
        subdirectory: Option<Box<Path>>,
        /// The PEP 508 style url in the format
        /// `git+<scheme>://<domain>/<path>@<rev>#subdirectory=<subdirectory>`.
        url: VerbatimUrl,
    },
    /// A remote Git archive, over either HTTPS or SSH.
    GitPath {
        /// The repository URL and reference to the commit to use.
        git: GitUrl,
        /// The path to the file in the repository.
        install_path: PathBuf,
        /// The file extension, e.g. `tar.gz`, `zip`, etc.
        ext: DistExtension,
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
    pub(crate) fn from_parsed_url(parsed_url: ParsedUrl, url: VerbatimUrl) -> Self {
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
            ParsedUrl::GitDirectory(git) => Self::GitDirectory {
                url,
                git: git.url,
                subdirectory: git.subdirectory,
            },
            ParsedUrl::GitPath(git) => Self::GitPath {
                url,
                git: git.url,
                install_path: git.install_path.clone(),
                ext: git.ext,
            },
            ParsedUrl::Archive(archive) => Self::Url {
                url,
                location: archive.url.with_policy(Raw),
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
                    location.as_url().clone(),
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
            Self::GitDirectory {
                git,
                subdirectory,
                url,
            } => Some(VerbatimParsedUrl {
                parsed_url: ParsedUrl::GitDirectory(ParsedGitDirectoryUrl::from_source(
                    git.clone(),
                    subdirectory.clone(),
                )),
                verbatim: url.clone(),
            }),
            Self::GitPath {
                git,
                install_path,
                ext,
                url,
            } => Some(VerbatimParsedUrl {
                parsed_url: ParsedUrl::GitPath(ParsedGitPathUrl::from_source(
                    git.clone(),
                    install_path.clone(),
                    *ext,
                )),
                verbatim: url.clone(),
            }),
        }
    }

    /// Returns `true` if the source is empty.
    pub fn is_empty(&self) -> bool {
        match self {
            Self::Registry { specifier, .. } => specifier.is_empty(),
            Self::Url { .. }
            | Self::GitPath { .. }
            | Self::GitDirectory { .. }
            | Self::Path { .. }
            | Self::Directory { .. } => false,
        }
    }

    /// If the source is the registry, return the version specifiers
    pub fn version_specifiers(&self) -> Option<&VersionSpecifiers> {
        match self {
            Self::Registry { specifier, .. } => Some(specifier),
            Self::Url { .. }
            | Self::GitPath { .. }
            | Self::GitDirectory { .. }
            | Self::Path { .. }
            | Self::Directory { .. } => None,
        }
    }

    /// Convert the source to a [`RequirementSource`] relative to the given path.
    fn relative_to(self, path: &Path) -> Result<Self, io::Error> {
        match self {
            Self::Registry { .. }
            | Self::Url { .. }
            | Self::GitPath { .. }
            | Self::GitDirectory { .. } => Ok(self),
            Self::Path {
                install_path,
                ext,
                url,
            } => Ok(Self::Path {
                install_path: try_relative_to_if(&install_path, path, url.prefers_relative())?
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
                install_path: try_relative_to_if(&install_path, path, url.prefers_relative())?
                    .into_boxed_path(),
                editable,
                r#virtual,
                url,
            }),
        }
    }

    /// Convert the source to a [`RequirementSource`] with an absolute path based on the given root.
    #[must_use]
    fn into_absolute(self, root: &Path) -> Self {
        match self {
            Self::Registry { .. }
            | Self::Url { .. }
            | Self::GitPath { .. }
            | Self::GitDirectory { .. } => self,
            Self::Path {
                install_path,
                ext,
                url,
            } => Self::Path {
                install_path: normalize_path(root.join(install_path))
                    .into_owned()
                    .into_boxed_path(),
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
                install_path: normalize_path(root.join(install_path))
                    .into_owned()
                    .into_boxed_path(),
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
            Self::GitDirectory {
                url: _,
                git,
                subdirectory,
            } => {
                write!(f, " git+{}", git.url())?;
                if let Some(reference) = git.reference().as_url_rev() {
                    write!(f, "@{reference}")?;
                }
                if let Some(subdirectory) = subdirectory {
                    writeln!(f, "#subdirectory={}", subdirectory.display())?;
                }
                if git.lfs().enabled() {
                    writeln!(
                        f,
                        "{}lfs=true",
                        if subdirectory.is_some() { "&" } else { "#" }
                    )?;
                }
            }
            Self::GitPath {
                url: _,
                git,
                install_path,
                ext: _,
            } => {
                write!(f, " git+{}", git.url())?;
                if let Some(reference) = git.reference().as_url_rev() {
                    write!(f, "@{reference}")?;
                }
                write!(f, "#path={}", install_path.display())?;
                if git.lfs().enabled() {
                    write!(f, "&lfs=true")?;
                }
                writeln!(f)?;
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

/// Read the flattened source fields once. A requirement can omit all of them to use the registry.
#[derive(serde::Deserialize)]
struct RequirementSourceWire {
    git: Option<String>,
    url: Option<SafeUrl<Raw>>,
    subdirectory: Option<PortablePathBuf>,
    path: Option<PortablePathBuf>,
    directory: Option<PortablePathBuf>,
    editable: Option<PortablePathBuf>,
    r#virtual: Option<PortablePathBuf>,
    #[serde(default)]
    specifier: VersionSpecifiers,
    index: Option<SafeUrl<Stripped>>,
    conflict: Option<ConflictItem>,
}

impl Serialize for RequirementSource {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Registry {
                specifier,
                index,
                conflict,
            } => {
                let mut state = serializer.serialize_struct(
                    "RequirementSourceWire",
                    2 + usize::from(!specifier.is_empty()),
                )?;
                if !specifier.is_empty() {
                    state.serialize_field("specifier", specifier)?;
                }
                state.serialize_field("index", &index.as_ref().map(|index| &index.url))?;
                state.serialize_field("conflict", conflict)?;
                state.end()
            }
            Self::Url {
                subdirectory,
                location,
                ext: _,
                url: _,
            } => {
                let mut state = serializer.serialize_struct("RequirementSourceWire", 2)?;
                state.serialize_field("url", location)?;
                state.serialize_field(
                    "subdirectory",
                    &subdirectory.as_deref().map(PortablePath::from),
                )?;
                state.end()
            }
            Self::GitDirectory {
                git, subdirectory, ..
            } => serialize_source_field(
                serializer,
                "git",
                &requirement_git_url(git, subdirectory.as_deref(), None),
            ),
            Self::GitPath {
                git, install_path, ..
            } => serialize_source_field(
                serializer,
                "git",
                &requirement_git_url(git, None, Some(install_path)),
            ),
            Self::Path {
                install_path,
                ext: _,
                url: _,
            } => serialize_source_field(serializer, "path", &PortablePath::from(install_path)),
            Self::Directory {
                install_path,
                editable,
                r#virtual,
                url: _,
            } => {
                let field = if editable.unwrap_or(false) {
                    "editable"
                } else if r#virtual.unwrap_or(false) {
                    "virtual"
                } else {
                    "directory"
                };
                serialize_source_field(serializer, field, &PortablePath::from(install_path))
            }
        }
    }
}

/// Encode a Git requirement's reference, location within the repository, and resolved commit.
fn requirement_git_url(git: &GitUrl, subdirectory: Option<&Path>, path: Option<&Path>) -> String {
    let mut url = git.url().clone();
    url.remove_credentials();
    url.set_query(None);
    url.set_fragment(git.resolved_commit().as_ref().map(GitOid::as_str));

    let reference = match git.reference() {
        GitReference::Branch(branch) => Some(("branch", branch.as_str())),
        GitReference::Tag(tag) => Some(("tag", tag.as_str())),
        GitReference::Commit(commit) => Some(("rev", commit.as_str())),
        GitReference::BranchOrTag(rev)
        | GitReference::BranchOrTagOrCommit(rev)
        | GitReference::NamedRef(rev) => Some(("rev", rev.as_str())),
        GitReference::DefaultBranch => None,
    };
    let lfs = git.lfs().enabled().then_some(("lfs", "true"));
    let subdirectory = subdirectory.map(|path| PortablePath::from(path).to_string());
    // The lockfile places the reference before `lfs` for Git archives and after it for directories.
    let query = if let Some(path) = path {
        [path.to_str().map(|path| ("path", path)), reference, lfs]
    } else {
        [
            subdirectory
                .as_deref()
                .map(|subdirectory| ("subdirectory", subdirectory)),
            lfs,
            reference,
        ]
    };
    for (key, value) in query.into_iter().flatten() {
        url.query_pairs_mut().append_pair(key, value);
    }
    url.to_string()
}

fn serialize_source_field<S: Serializer>(
    serializer: S,
    key: &'static str,
    value: &impl Serialize,
) -> Result<S::Ok, S::Error> {
    let mut state = serializer.serialize_struct("RequirementSourceWire", 1)?;
    state.serialize_field(key, value)?;
    state.end()
}

impl TryFrom<RequirementSourceWire> for RequirementSource {
    type Error = RequirementError;

    fn try_from(wire: RequirementSourceWire) -> Result<Self, RequirementError> {
        match wire {
            RequirementSourceWire { git: Some(git), .. } => {
                let mut repository = SafeUrl::parse(&git)?;

                let mut reference = GitReference::DefaultBranch;
                let mut subdirectory: Option<PortablePathBuf> = None;
                let mut lfs = GitLfs::Disabled;
                let mut path: Option<PortablePathBuf> = None;
                for (key, val) in repository.query_pairs() {
                    match &*key {
                        "tag" => reference = GitReference::Tag(val.into_owned()),
                        "branch" => reference = GitReference::Branch(val.into_owned()),
                        "rev" => reference = GitReference::from_rev(val.into_owned()),
                        "subdirectory" => {
                            subdirectory = Some(PortablePathBuf::from(val.as_ref()));
                        }
                        "lfs" => lfs = GitLfs::from(val.eq_ignore_ascii_case("true")),
                        "path" => {
                            path = Some(PortablePathBuf::from(val.as_ref()));
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
                let mut url = SafeUrl::parse(&format!("git+{repository}"))?;
                if let Some(rev) = reference.as_url_rev() {
                    let path = format!("{}@{}", url.path(), rev);
                    url.set_path(&path);
                }
                let mut frags: Vec<String> = Vec::new();
                if let Some(subdirectory) = subdirectory.as_ref() {
                    frags.push(format!("subdirectory={subdirectory}"));
                }
                // Preserve that we're using Git LFS in the Verbatim Url representations
                if lfs.enabled() {
                    frags.push("lfs=true".to_string());
                }
                if let Some(path) = path.as_ref() {
                    frags.push(format!("path={path}"));
                }
                if !frags.is_empty() {
                    url.set_fragment(Some(&frags.join("&")));
                }
                let url = VerbatimUrl::from_url(url);
                let git = GitUrl::from_fields(repository, reference, precise, lfs)?;

                if let Some(install_path) = path.map(Box::<Path>::from).map(PathBuf::from) {
                    Ok(Self::GitPath {
                        git,
                        ext: DistExtension::from_path(install_path.as_path()).map_err(|err| {
                            ParsedUrlError::MissingExtensionPath(install_path.clone(), err)
                        })?,
                        install_path,
                        url,
                    })
                } else {
                    Ok(Self::GitDirectory {
                        git,
                        subdirectory: subdirectory.map(Box::<Path>::from),
                        url,
                    })
                }
            }
            RequirementSourceWire {
                url: Some(url),
                subdirectory,
                ..
            } => {
                let location = url.clone();

                // Create a PEP 508-compatible URL.
                let mut url = url.into_url();
                if let Some(subdirectory) = &subdirectory {
                    url.set_fragment(Some(&format!("subdirectory={subdirectory}")));
                }

                Ok(Self::Url {
                    location,
                    subdirectory: subdirectory.map(Box::<Path>::from),
                    ext: DistExtension::from_path(url.path())
                        .map_err(|err| ParsedUrlError::MissingExtensionUrl(url.to_string(), err))?,
                    url: VerbatimUrl::from_url(url),
                })
            }
            // TODO(charlie): The use of `CWD` here is incorrect. These should be resolved relative
            // to the workspace root, but we don't have access to it here. When comparing these
            // sources in the lockfile, we replace the URL anyway. Ideally, we'd either remove the
            // URL field or make it optional.
            RequirementSourceWire {
                path: Some(path), ..
            } => {
                let path = Box::<Path>::from(path);
                let url = VerbatimUrl::from_normalized_path(normalize_path(CWD.join(&path)))?;
                Ok(Self::Path {
                    ext: DistExtension::from_path(&path).map_err(|err| {
                        ParsedUrlError::MissingExtensionPath(path.to_path_buf(), err)
                    })?,
                    install_path: path,
                    url,
                })
            }
            RequirementSourceWire {
                directory: Some(directory),
                ..
            } => Self::from_wire_directory(directory, false, false),
            RequirementSourceWire {
                editable: Some(directory),
                ..
            } => Self::from_wire_directory(directory, true, false),
            RequirementSourceWire {
                r#virtual: Some(directory),
                ..
            } => Self::from_wire_directory(directory, false, true),
            RequirementSourceWire {
                specifier,
                index,
                conflict,
                ..
            } => Ok(Self::Registry {
                specifier,
                index: index
                    .map(|index| IndexMetadata::from(IndexUrl::from(VerbatimUrl::from_url(index)))),
                conflict,
            }),
        }
    }
}

impl RequirementSource {
    fn from_wire_directory(
        directory: PortablePathBuf,
        editable: bool,
        r#virtual: bool,
    ) -> Result<Self, RequirementError> {
        let install_path = Box::<Path>::from(directory);
        let url = VerbatimUrl::from_normalized_path(normalize_path(CWD.join(&install_path)))?;
        Ok(Self::Directory {
            install_path,
            editable: Some(editable),
            r#virtual: Some(r#virtual),
            url,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error;
    use std::path::PathBuf;

    use uv_pep508::{MarkerTree, VerbatimUrl};

    use crate::{Requirement, RequirementScope, RequirementSource};

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
            scope: RequirementScope::Global,
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
            scope: RequirementScope::Global,
            origin: None,
        };

        let raw = toml::to_string(&requirement).unwrap();
        let deserialized: Requirement = toml::from_str(&raw).unwrap();
        assert_eq!(requirement, deserialized);
    }

    /// Only the optional fields belonging to the selected source appear as null in JSON.
    #[test]
    fn serialize_optional_source_fields() -> Result<(), Box<dyn Error>> {
        let registry: RequirementSource = toml::from_str("")?;
        assert_eq!(
            serde_json::to_string(&registry)?,
            r#"{"index":null,"conflict":null}"#
        );

        let direct: RequirementSource =
            toml::from_str(r#"url = "https://example.org/foo-1.0.zip""#)?;
        assert_eq!(
            serde_json::to_string(&direct)?,
            r#"{"url":"https://example.org/foo-1.0.zip","subdirectory":null}"#
        );
        Ok(())
    }

    /// Falling back after a Git type error would turn this into an unrestricted registry requirement.
    #[test]
    fn invalid_git_source_is_rejected() {
        let error = toml::from_str::<Requirement>("name = 'example'\ngit = false")
            .expect_err("a Git source must be a string");
        insta::assert_snapshot!(error.message(), @"invalid type: boolean `false`, expected a string");
    }

    #[test]
    fn roundtrip_git_commit() -> Result<(), Box<dyn Error>> {
        let raw = "git = \"https://example.com/repository?rev=0dacfd662c64cb4ceb16e6cf65a157a8b715b979\"\n";
        let source: RequirementSource = toml::from_str(raw)?;
        assert_eq!(toml::to_string(&source)?, raw);

        // A redundant precise fragment denotes the same requirement as the full commit revision.
        let resolved: RequirementSource = toml::from_str(
            "git = \"https://example.com/repository?rev=0dacfd662c64cb4ceb16e6cf65a157a8b715b979#0dacfd662c64cb4ceb16e6cf65a157a8b715b979\"",
        )?;
        assert_eq!(source, resolved);

        Ok(())
    }

    #[test]
    fn display_git_path_lfs() {
        let source: RequirementSource = toml::from_str(
            r#"git = "https://github.com/astral-sh/archive-in-git-test?lfs=true&path=archives%2Finiconfig-2.0.0-py3-none-any.whl""#,
        )
        .unwrap();

        assert_eq!(
            source.to_string(),
            " git+https://github.com/astral-sh/archive-in-git-test#path=archives/iniconfig-2.0.0-py3-none-any.whl&lfs=true\n"
        );

        let requirement = Requirement {
            name: "iniconfig".parse().unwrap(),
            extras: Box::new([]),
            groups: Box::new([]),
            marker: MarkerTree::TRUE,
            source,
            scope: RequirementScope::Global,
            origin: None,
        };
        assert_eq!(
            requirement.to_string(),
            "iniconfig @ git+https://github.com/astral-sh/archive-in-git-test#path=archives/iniconfig-2.0.0-py3-none-any.whl&lfs=true\n"
        );
    }
}
