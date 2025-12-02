pub use crate::github::GitHubRepository;
pub use crate::oid::{GitOid, OidParseError};
pub use crate::reference::GitReference;
use std::sync::LazyLock;

use thiserror::Error;
use uv_redacted::DisplaySafeUrl;
use uv_static::EnvVars;

mod github;
mod oid;
mod reference;

/// Initialize [`GitLfs`] mode from `UV_GIT_LFS` environment.
pub static UV_GIT_LFS: LazyLock<GitLfs> = LazyLock::new(|| {
    // TODO(konsti): Parse this in `EnvironmentOptions`.
    if std::env::var_os(EnvVars::UV_GIT_LFS)
        .and_then(|v| v.to_str().map(str::to_lowercase))
        .is_some_and(|v| matches!(v.as_str(), "y" | "yes" | "t" | "true" | "on" | "1"))
    {
        GitLfs::Enabled
    } else {
        GitLfs::Disabled
    }
});

/// Configuration for Git LFS (Large File Storage) support.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub enum GitLfs {
    /// Git LFS is disabled (default).
    #[default]
    Disabled,
    /// Git LFS is enabled.
    Enabled,
}

impl GitLfs {
    /// Create a `GitLfs` configuration from environment variables.
    pub fn from_env() -> Self {
        *UV_GIT_LFS
    }

    /// Returns true if LFS is enabled.
    pub fn enabled(self) -> bool {
        matches!(self, Self::Enabled)
    }
}

impl From<Option<bool>> for GitLfs {
    fn from(value: Option<bool>) -> Self {
        match value {
            Some(true) => Self::Enabled,
            Some(false) => Self::Disabled,
            None => Self::from_env(),
        }
    }
}

impl From<bool> for GitLfs {
    fn from(value: bool) -> Self {
        if value { Self::Enabled } else { Self::Disabled }
    }
}

impl std::fmt::Display for GitLfs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Enabled => write!(f, "enabled"),
            Self::Disabled => write!(f, "disabled"),
        }
    }
}

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
    /// Git LFS configuration for this repository.
    lfs: GitLfs,
}

impl GitUrl {
    /// Create a new [`GitUrl`] from a repository URL and a reference.
    pub fn from_reference(
        repository: DisplaySafeUrl,
        reference: GitReference,
        lfs: GitLfs,
    ) -> Result<Self, GitUrlParseError> {
        Self::from_fields(repository, reference, None, lfs)
    }

    /// Create a new [`GitUrl`] from a repository URL and a precise commit.
    pub fn from_commit(
        repository: DisplaySafeUrl,
        reference: GitReference,
        precise: GitOid,
        lfs: GitLfs,
    ) -> Result<Self, GitUrlParseError> {
        Self::from_fields(repository, reference, Some(precise), lfs)
    }

    /// Create a new [`GitUrl`] from a repository URL and a precise commit, if known.
    pub fn from_fields(
        repository: DisplaySafeUrl,
        reference: GitReference,
        precise: Option<GitOid>,
        lfs: GitLfs,
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
            lfs,
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

    /// Return the Git LFS configuration.
    pub fn lfs(&self) -> GitLfs {
        self.lfs
    }

    /// Set the Git LFS configuration.
    #[must_use]
    pub fn with_lfs(mut self, lfs: GitLfs) -> Self {
        self.lfs = lfs;
        self
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

        // TODO(samypr100): GitLfs::from_env() for now unless we want to support parsing lfs=true
        Self::from_reference(url, reference, GitLfs::from_env())
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
