use std::str::FromStr;
use url::Url;

use crate::git::GitReference;
pub use crate::sha::GitSha;
pub use crate::source::{Fetch, GitSource, Reporter};

mod git;
mod known_hosts;
mod sha;
mod source;
mod util;

/// A URL reference to a Git repository.
#[derive(Debug, Clone)]
pub struct GitUrl {
    /// The URL of the Git repository, with any query parameters and fragments removed.
    repository: Url,
    /// The reference to the commit to use, which could be a branch, tag or revision.
    reference: GitReference,
    /// The precise commit to use, if known.
    precise: Option<GitSha>,
}

impl GitUrl {
    #[must_use]
    pub(crate) fn with_precise(mut self, precise: GitSha) -> Self {
        self.precise = Some(precise);
        self
    }

    /// Return the [`Url`] of the Git repository.
    pub fn repository(&self) -> &Url {
        &self.repository
    }

    /// Return the reference to the commit to use, which could be a branch, tag or revision.
    pub fn reference(&self) -> Option<&str> {
        match &self.reference {
            GitReference::Branch(rev)
            | GitReference::Tag(rev)
            | GitReference::BranchOrTag(rev)
            | GitReference::Ref(rev)
            | GitReference::FullCommit(rev)
            | GitReference::ShortCommit(rev) => Some(rev),
            GitReference::DefaultBranch => None,
        }
    }

    /// Return the precise commit, if known.
    pub fn precise(&self) -> Option<GitSha> {
        self.precise
    }
}

impl TryFrom<Url> for GitUrl {
    type Error = anyhow::Error;

    /// Initialize a [`GitUrl`] source from a URL.
    fn try_from(mut url: Url) -> Result<Self, Self::Error> {
        // Remove any query parameters and fragments.
        url.set_fragment(None);
        url.set_query(None);

        // If the URL ends with a reference, like `https://git.example.com/MyProject.git@v1.0`,
        // extract it.
        let mut reference = GitReference::DefaultBranch;
        if let Some((prefix, rev)) = url.as_str().rsplit_once('@') {
            reference = GitReference::from_rev(rev);
            url = Url::parse(prefix)?;
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
                | GitReference::BranchOrTag(rev)
                | GitReference::Ref(rev)
                | GitReference::FullCommit(rev)
                | GitReference::ShortCommit(rev) => {
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

#[derive(Debug, Clone, Copy)]
pub enum FetchStrategy {
    /// Fetch Git repositories using libgit2.
    Libgit2,
    /// Fetch Git repositories using the `git` CLI.
    Cli,
}
