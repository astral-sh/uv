use std::borrow::Cow;
use std::path::Path;

use url::Url;

use uv_normalize::PackageName;

use crate::{GitSourceDist, Name, PathSourceDist, SourceDist};

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

    /// Return the [`BuildableSource`] as a [`SourceDist`], if it is a distribution.
    pub fn as_dist(&self) -> Option<&SourceDist> {
        match self {
            Self::Dist(dist) => Some(dist),
            Self::Url(_) => None,
        }
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
}

impl std::fmt::Display for SourceUrl<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Direct(url) => write!(f, "{url}"),
            Self::Git(url) => write!(f, "{url}"),
            Self::Path(url) => write!(f, "{url}"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct DirectSourceUrl<'a> {
    pub url: &'a Url,
}

impl std::fmt::Display for DirectSourceUrl<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{url}", url = self.url)
    }
}

#[derive(Debug, Clone)]
pub struct GitSourceUrl<'a> {
    pub url: &'a Url,
}

impl std::fmt::Display for GitSourceUrl<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{url}", url = self.url)
    }
}

impl<'a> From<&'a GitSourceDist> for GitSourceUrl<'a> {
    fn from(dist: &'a GitSourceDist) -> Self {
        Self { url: &dist.url }
    }
}

#[derive(Debug, Clone)]
pub struct PathSourceUrl<'a> {
    pub url: &'a Url,
    pub path: Cow<'a, Path>,
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
            path: Cow::Borrowed(&dist.path),
        }
    }
}
