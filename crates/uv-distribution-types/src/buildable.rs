use std::borrow::Cow;
use std::path::Path;

use url::Url;
use uv_distribution_filename::SourceDistExtension;
use uv_git_types::GitUrl;
use uv_pep440::{Version, VersionSpecifiers};
use uv_pep508::VerbatimUrl;

use uv_normalize::PackageName;

use crate::{DirectorySourceDist, GitSourceDist, Name, PathSourceDist, SourceDist};

/// A reference to a source that can be built into a built distribution.
///
/// This can either be a distribution (e.g., a package on a registry) or a direct URL.
///
/// Distributions can _also_ point to URLs in lieu of a registry; however, the primary distinction
/// here is that a distribution will always include a package name, while a URL will not.
#[derive(Debug, Clone)]
pub enum BuildableSource<'a> {
    Dist(&'a SourceDist),
    Url(SourceUrl<'a>),
}

impl BuildableSource<'_> {
    /// Return the [`PackageName`] of the source, if available.
    pub fn name(&self) -> Option<&PackageName> {
        match self {
            Self::Dist(dist) => Some(dist.name()),
            Self::Url(_) => None,
        }
    }

    /// Return the source tree of the source, if available.
    pub fn source_tree(&self) -> Option<&Path> {
        match self {
            Self::Dist(dist) => dist.source_tree(),
            Self::Url(url) => url.source_tree(),
        }
    }

    /// Return the [`Version`] of the source, if available.
    pub fn version(&self) -> Option<&Version> {
        match self {
            Self::Dist(SourceDist::Registry(dist)) => Some(&dist.version),
            Self::Dist(SourceDist::Path(dist)) => dist.version.as_ref(),
            Self::Dist(_) => None,
            Self::Url(_) => None,
        }
    }

    /// Return the [`BuildableSource`] as a [`SourceDist`], if it is a distribution.
    pub fn as_dist(&self) -> Option<&SourceDist> {
        match self {
            Self::Dist(dist) => Some(dist),
            Self::Url(_) => None,
        }
    }

    /// Returns `true` if the source is editable.
    pub fn is_editable(&self) -> bool {
        match self {
            Self::Dist(dist) => dist.is_editable(),
            Self::Url(url) => url.is_editable(),
        }
    }

    /// Return true if the source refers to a local source tree (i.e., a directory).
    pub fn is_source_tree(&self) -> bool {
        match self {
            Self::Dist(dist) => matches!(dist, SourceDist::Directory(_)),
            Self::Url(url) => matches!(url, SourceUrl::Directory(_)),
        }
    }

    /// Return the Python version specifier required by the source, if available.
    pub fn requires_python(&self) -> Option<&VersionSpecifiers> {
        let Self::Dist(SourceDist::Registry(dist)) = self else {
            return None;
        };
        dist.file.requires_python.as_ref()
    }
}

impl std::fmt::Display for BuildableSource<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Dist(dist) => write!(f, "{dist}"),
            Self::Url(url) => write!(f, "{url}"),
        }
    }
}

/// A reference to a source distribution defined by a URL.
#[derive(Debug, Clone)]
pub enum SourceUrl<'a> {
    Direct(DirectSourceUrl<'a>),
    Git(GitSourceUrl<'a>),
    Path(PathSourceUrl<'a>),
    Directory(DirectorySourceUrl<'a>),
}

impl SourceUrl<'_> {
    /// Return the [`Url`] of the source.
    pub fn url(&self) -> &Url {
        match self {
            Self::Direct(dist) => dist.url,
            Self::Git(dist) => dist.url,
            Self::Path(dist) => dist.url,
            Self::Directory(dist) => dist.url,
        }
    }

    /// Return the source tree of the source, if available.
    pub fn source_tree(&self) -> Option<&Path> {
        match self {
            Self::Directory(dist) => Some(&dist.install_path),
            _ => None,
        }
    }

    /// Returns `true` if the source is editable.
    pub fn is_editable(&self) -> bool {
        matches!(
            self,
            Self::Directory(DirectorySourceUrl { editable: true, .. })
        )
    }

    /// Return true if the source refers to a local file or directory.
    pub fn is_local(&self) -> bool {
        matches!(self, Self::Path(_) | Self::Directory(_))
    }
}

impl std::fmt::Display for SourceUrl<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Direct(url) => write!(f, "{url}"),
            Self::Git(url) => write!(f, "{url}"),
            Self::Path(url) => write!(f, "{url}"),
            Self::Directory(url) => write!(f, "{url}"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct DirectSourceUrl<'a> {
    pub url: &'a Url,
    pub subdirectory: Option<&'a Path>,
    pub ext: SourceDistExtension,
}

impl std::fmt::Display for DirectSourceUrl<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{url}", url = self.url)
    }
}

#[derive(Debug, Clone)]
pub struct GitSourceUrl<'a> {
    /// The URL with the revision and subdirectory fragment.
    pub url: &'a VerbatimUrl,
    pub git: &'a GitUrl,
    /// The URL without the revision and subdirectory fragment.
    pub subdirectory: Option<&'a Path>,
}

impl std::fmt::Display for GitSourceUrl<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{url}", url = self.url)
    }
}

impl<'a> From<&'a GitSourceDist> for GitSourceUrl<'a> {
    fn from(dist: &'a GitSourceDist) -> Self {
        Self {
            url: &dist.url,
            git: &dist.git,
            subdirectory: dist.subdirectory.as_deref(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PathSourceUrl<'a> {
    pub url: &'a Url,
    pub path: Cow<'a, Path>,
    pub ext: SourceDistExtension,
}

impl std::fmt::Display for PathSourceUrl<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{url}", url = self.url)
    }
}

impl<'a> From<&'a PathSourceDist> for PathSourceUrl<'a> {
    fn from(dist: &'a PathSourceDist) -> Self {
        Self {
            url: &dist.url,
            path: Cow::Borrowed(&dist.install_path),
            ext: dist.ext,
        }
    }
}

#[derive(Debug, Clone)]
pub struct DirectorySourceUrl<'a> {
    pub url: &'a Url,
    pub install_path: Cow<'a, Path>,
    pub editable: bool,
}

impl std::fmt::Display for DirectorySourceUrl<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{url}", url = self.url)
    }
}

impl<'a> From<&'a DirectorySourceDist> for DirectorySourceUrl<'a> {
    fn from(dist: &'a DirectorySourceDist) -> Self {
        Self {
            url: &dist.url,
            install_path: Cow::Borrowed(&dist.install_path),
            editable: dist.editable,
        }
    }
}
