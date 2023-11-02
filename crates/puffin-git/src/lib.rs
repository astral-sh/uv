use url::Url;

use crate::git::GitReference;
pub use crate::source::GitSource;

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

impl Git {
    #[must_use]
    pub(crate) fn with_precise(mut self, precise: git2::Oid) -> Self {
        self.precise = Some(precise);
        self
    }
}

impl TryFrom<Url> for Git {
    type Error = anyhow::Error;

    /// Initialize a [`Git`] source from a URL.
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

        Ok(Self {
            url,
            reference,
            precise: None,
        })
    }
}

impl From<Git> for Url {
    fn from(git: Git) -> Self {
        let mut url = git.url;

        // If we have a precise commit, add `@` and the commit hash to the URL.
        if let Some(precise) = git.precise {
            url.set_path(&format!("{}@{}", url.path(), precise));
        } else {
            // Otherwise, add the branch or tag name.
            match git.reference {
                GitReference::Branch(rev)
                | GitReference::Tag(rev)
                | GitReference::BranchOrTag(rev)
                | GitReference::Rev(rev) => {
                    url.set_path(&format!("{}@{}", url.path(), rev));
                }
                GitReference::DefaultBranch => {}
            }
        }

        url
    }
}

impl std::fmt::Display for Git {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.url)
    }
}

#[derive(Debug, Clone, Copy)]
pub enum FetchStrategy {
    /// Fetch Git repositories using libgit2.
    Libgit2,
    /// Fetch Git repositories using the `git` CLI.
    Cli,
}
