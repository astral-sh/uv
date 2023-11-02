use std::borrow::Cow;

use anyhow::{Error, Result};
use url::Url;

use puffin_distribution::RemoteDistributionRef;
use puffin_git::Git;

/// The source of a distribution.
#[derive(Debug)]
pub(crate) enum Source<'a> {
    /// The distribution is available at a remote URL. This could be a dedicated URL, or a URL
    /// served by a registry, like PyPI.
    Url(Cow<'a, Url>),
    /// The distribution is available in a remote Git repository.
    Git(Git),
}

impl<'a> TryFrom<&'a RemoteDistributionRef<'_>> for Source<'a> {
    type Error = Error;

    fn try_from(value: &'a RemoteDistributionRef<'_>) -> Result<Self, Self::Error> {
        match value {
            // If a distribution is hosted on a registry, it must be available at a URL.
            RemoteDistributionRef::Registry(_, _, file) => {
                let url = Url::parse(&file.url)?;
                Ok(Self::Url(Cow::Owned(url)))
            }
            // If a distribution is specified via a direct URL, it could be a URL to a hosted file,
            // or a URL to a Git repository.
            RemoteDistributionRef::Url(_, url) => {
                if let Some(url) = url.as_str().strip_prefix("git+") {
                    let url = Url::parse(url)?;
                    let git = Git::try_from(url)?;
                    Ok(Self::Git(git))
                } else {
                    Ok(Self::Url(Cow::Borrowed(url)))
                }
            }
        }
    }
}
