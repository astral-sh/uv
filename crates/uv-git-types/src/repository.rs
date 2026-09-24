use uv_cache_key::RepositoryUrl;

use crate::{GitOid, GitReference, GitUrl};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ResolvedRepositoryReference {
    /// An abstract reference to a Git repository, including the URL and the commit (e.g., a branch,
    /// tag, or revision).
    pub reference: RepositoryReference,
    /// The precise commit SHA of the reference.
    pub sha: GitOid,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RepositoryReference {
    /// The URL of the Git repository, with any query parameters and fragments removed.
    pub url: RepositoryUrl,
    /// The reference to the commit to use, which could be a branch, tag, or revision.
    pub reference: GitReference,
}

impl From<&GitUrl> for RepositoryReference {
    fn from(git: &GitUrl) -> Self {
        Self {
            url: git.repository().clone(),
            reference: git.reference().clone(),
        }
    }
}
