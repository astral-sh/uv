use anyhow::Result;
use url::Url;

use pep440_rs::Version;
use puffin_cache::CanonicalUrl;
use puffin_distribution::RemoteDistributionRef;
use puffin_git::{GitSource, ResolvedGitSource};
use puffin_normalize::PackageName;
use puffin_package::pypi_types::File;
use puffin_traits::BuildContext;

use crate::distribution::source::Source;

/// Like [`RemoteDistributionRef`], but with a precise version.
pub enum Precise<'a> {
    /// The distribution exists in a registry, like `PyPI`.
    Registry(&'a PackageName, &'a Version, &'a File),
    /// The distribution exists at an arbitrary URL.
    Url(&'a PackageName, &'a Url),
    /// The distribution exists as a Git repository.
    Git(&'a PackageName, ResolvedGitSource),
}

impl<'a> Precise<'a> {
    /// Initialize a [`Precise`] from a [`RemoteDistributionRef`].
    pub fn try_from(
        distribution: &'a RemoteDistributionRef<'_>,
        context: &impl BuildContext,
    ) -> Result<Self> {
        match distribution {
            RemoteDistributionRef::Registry(name, version, file) => {
                Ok(Self::Registry(name, version, file))
            }
            RemoteDistributionRef::Url(name, url) => {
                let source = Source::try_from(distribution)?;
                match source {
                    Source::Url(url) => Ok(Self::Url(name, &url)),
                    Source::Git(git) => {
                        let git_dir = context.cache().join("git-v0");
                        let source = GitSource::new(git, git_dir);
                        Ok(Self::Git(name, source.fetch()?))
                    }
                }
            }
        }
    }

    pub fn id(&self) -> String {
        match self {
            Self::Registry(name, version, _) => {
                format!("{}-{}", PackageName::from(*name), version)
            }
            Self::Url(_name, url) => {
                puffin_cache::digest(&CanonicalUrl::new(url))
            }
            Self::Git(name, source) => {
                // source.
            }
        }
    }
}

impl std::fmt::Display for Precise<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Registry(name, version, _) => write!(f, "{}-{}", name, version),
            Self::Url(_name, url) => write!(f, "{}", url),
            Self::Git(_name, git) => write!(f, "{}", git),
        }
    }
}
