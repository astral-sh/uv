use std::str::FromStr;
use url::Url;

pub use crate::git::GitReference;
pub use crate::resolver::{
    GitResolver, GitResolverError, RepositoryReference, ResolvedRepositoryReference,
};
pub use crate::sha::{GitOid, GitSha, OidParseError};
pub use crate::source::{Fetch, GitSource, Reporter};

mod git;
mod resolver;
mod sha;
mod source;

/// A URL reference to a Git repository.
#[derive(Debug, Clone, PartialEq, PartialOrd, Eq, Hash, Ord)]
pub struct GitUrl {
    /// The URL of the Git repository, with any query parameters, fragments, and leading `git+`
    /// removed.
    repository: Url,
    /// The reference to the commit to use, which could be a branch, tag or revision.
    reference: GitReference,
    /// The precise commit to use, if known.
    precise: Option<GitSha>,
}

impl GitUrl {
    pub fn new(repository: Url, reference: GitReference) -> Self {
        Self {
            repository,
            reference,
            precise: None,
        }
    }

    #[must_use]
    pub fn with_precise(mut self, precise: GitSha) -> Self {
        self.precise = Some(precise);
        self
    }

    /// Return the [`Url`] of the Git repository.
    pub fn repository(&self) -> &Url {
        &self.repository
    }

    /// Return the reference to the commit to use, which could be a branch, tag or revision.
    pub fn reference(&self) -> &GitReference {
        &self.reference
    }

    /// Return the precise commit, if known.
    pub fn precise(&self) -> Option<GitSha> {
        self.precise
    }
}

impl TryFrom<Url> for GitUrl {
    type Error = OidParseError;

    /// Initialize a [`GitUrl`] source from a URL.
    fn try_from(mut url: Url) -> Result<Self, Self::Error> {
        // Remove any query parameters and fragments.
        url.set_fragment(None);
        url.set_query(None);

        // If the URL ends with a reference, like `https://git.example.com/MyProject.git@v1.0`,
        // extract it.
        let mut reference = GitReference::DefaultBranch;
        if let Some((prefix, suffix)) = url
            .path()
            .rsplit_once('@')
            .map(|(prefix, suffix)| (prefix.to_string(), suffix.to_string()))
        {
            reference = GitReference::from_rev(suffix);
            url.set_path(&prefix);
        }

        let precise = if let GitReference::FullCommit(rev) = &reference {
            Some(GitSha::from_str(rev)?)
        } else {
            None
        };

        Ok(Self {
            repository: url,
            reference,
            precise,
        })
    }
}

impl From<GitUrl> for Url {
    fn from(git: GitUrl) -> Self {
        let mut url = git.repository;

        // If we have a precise commit, add `@` and the commit hash to the URL.
        if let Some(precise) = git.precise {
            url.set_path(&format!("{}@{}", url.path(), precise));
        } else {
            // Otherwise, add the branch or tag name.
            match git.reference {
                GitReference::Branch(rev)
                | GitReference::Tag(rev)
                | GitReference::ShortCommit(rev)
                | GitReference::BranchOrTag(rev)
                | GitReference::NamedRef(rev)
                | GitReference::FullCommit(rev)
                | GitReference::BranchOrTagOrCommit(rev) => {
                    url.set_path(&format!("{}@{}", url.path(), rev));
                }
                GitReference::DefaultBranch => {}
            }
        }

        url
    }
}

impl std::fmt::Display for GitUrl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.repository)
    }
}
