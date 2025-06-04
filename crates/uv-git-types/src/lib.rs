pub use crate::github::GitHubRepository;
pub use crate::oid::{GitOid, OidParseError};
pub use crate::reference::GitReference;

use thiserror::Error;
use uv_redacted::DisplaySafeUrl;

mod github;
mod oid;
mod reference;

#[derive(Debug, Error)]
pub enum GitUrlParseError {
    #[error(
        "Unsupported Git URL scheme `{0}:` in `{1}` (expected one of `https:`, `ssh:`, or `file:`)"
    )]
    UnsupportedGitScheme(String, DisplaySafeUrl),
}

/// A URL reference to a Git repository.
#[derive(Debug, Clone, PartialEq, PartialOrd, Eq, Hash, Ord)]
pub struct GitUrl {
    /// The URL of the Git repository, with any query parameters, fragments, and leading `git+`
    /// removed.
    repository: DisplaySafeUrl,
    /// The reference to the commit to use, which could be a branch, tag or revision.
    reference: GitReference,
    /// The precise commit to use, if known.
    precise: Option<GitOid>,
}

impl GitUrl {
    /// Create a new [`GitUrl`] from a repository URL and a reference.
    pub fn from_reference(
        repository: DisplaySafeUrl,
        reference: GitReference,
    ) -> Result<Self, GitUrlParseError> {
        Self::from_fields(repository, reference, None)
    }

    /// Create a new [`GitUrl`] from a repository URL and a precise commit.
    pub fn from_commit(
        repository: DisplaySafeUrl,
        reference: GitReference,
        precise: GitOid,
    ) -> Result<Self, GitUrlParseError> {
        Self::from_fields(repository, reference, Some(precise))
    }

    /// Create a new [`GitUrl`] from a repository URL and a precise commit, if known.
    pub fn from_fields(
        repository: DisplaySafeUrl,
        reference: GitReference,
        precise: Option<GitOid>,
    ) -> Result<Self, GitUrlParseError> {
        match repository.scheme() {
            "http" | "https" | "ssh" | "file" => {}
            unsupported => {
                return Err(GitUrlParseError::UnsupportedGitScheme(
                    unsupported.to_string(),
                    repository,
                ));
            }
        }
        Ok(Self {
            repository,
            reference,
            precise,
        })
    }

    /// Set the precise [`GitOid`] to use for this Git URL.
    #[must_use]
    pub fn with_precise(mut self, precise: GitOid) -> Self {
        self.precise = Some(precise);
        self
    }

    /// Set the [`GitReference`] to use for this Git URL.
    #[must_use]
    pub fn with_reference(mut self, reference: GitReference) -> Self {
        self.reference = reference;
        self
    }

    /// Return the [`Url`] of the Git repository.
    pub fn repository(&self) -> &DisplaySafeUrl {
        &self.repository
    }

    /// Return the reference to the commit to use, which could be a branch, tag or revision.
    pub fn reference(&self) -> &GitReference {
        &self.reference
    }

    /// Return the precise commit, if known.
    pub fn precise(&self) -> Option<GitOid> {
        self.precise
    }
}

impl TryFrom<DisplaySafeUrl> for GitUrl {
    type Error = GitUrlParseError;

    /// Initialize a [`GitUrl`] source from a URL.
    fn try_from(mut url: DisplaySafeUrl) -> Result<Self, Self::Error> {
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

        Self::from_reference(url, reference)
    }
}

impl From<GitUrl> for DisplaySafeUrl {
    fn from(git: GitUrl) -> Self {
        let mut url = git.repository;

        // If we have a precise commit, add `@` and the commit hash to the URL.
        if let Some(precise) = git.precise {
            let path = format!("{}@{}", url.path(), precise);
            url.set_path(&path);
        } else {
            // Otherwise, add the branch or tag name.
            match git.reference {
                GitReference::Branch(rev)
                | GitReference::Tag(rev)
                | GitReference::BranchOrTag(rev)
                | GitReference::NamedRef(rev)
                | GitReference::BranchOrTagOrCommit(rev) => {
                    let path = format!("{}@{}", url.path(), rev);
                    url.set_path(&path);
                }
                GitReference::DefaultBranch => {}
            }
        }

        url
    }
}

impl std::fmt::Display for GitUrl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", &self.repository)
    }
}
