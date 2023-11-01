//! Home of the [`GitSource`].
//!
//! Apparently, the most important type in this module is [`GitSource`].
//! [`git`] provides libgit2 utilities like fetch and checkout.

use url::Url;

pub use self::source::GitSource;

mod git;
mod source;
mod util;

/// A reference to a Git repository.
#[derive(Debug, Clone)]
pub struct Git {
    /// The URL of the Git repository, with any query parameters and fragments removed.
    url: Url,
    /// The reference to the commit to use, which could be a branch, tag or revision.
    reference: GitReference,
    /// The precise commit to use, if known.
    precise: Option<git2::Oid>,
}

impl TryFrom<Url> for Git {
    type Error = anyhow::Error;

    /// Initialize a [`Git`] source from a URL.
    fn try_from(mut url: Url) -> Result<Self, Self::Error> {
        let mut reference = GitReference::DefaultBranch;
        for (k, v) in url.query_pairs() {
            match &k[..] {
                // Map older 'ref' to branch.
                "branch" | "ref" => reference = GitReference::Branch(v.into_owned()),
                "rev" => reference = GitReference::Rev(v.into_owned()),
                "tag" => reference = GitReference::Tag(v.into_owned()),
                _ => {}
            }
        }
        let precise = url.fragment().map(git2::Oid::from_str).transpose()?;
        url.set_fragment(None);
        url.set_query(None);

        Ok(Self {
            url,
            reference,
            precise,
        })
    }
}

impl std::fmt::Display for Git {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.url)
    }
}

/// Information to find a specific commit in a Git repository.
#[derive(Debug, Clone)]
pub enum GitReference {
    /// From a tag.
    Tag(String),
    /// From a branch.
    Branch(String),
    /// From a specific revision. Can be a commit hash (either short or full),
    /// or a named reference like `refs/pull/493/head`.
    Rev(String),
    /// The default branch of the repository, the reference named `HEAD`.
    DefaultBranch,
}

#[derive(Debug, Clone, Copy)]
pub enum FetchStrategy {
    /// Fetch Git repositories using libgit2.
    Libgit2,
    /// Fetch Git repositories using the `git` CLI.
    Cli,
}
