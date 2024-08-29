use std::borrow::Cow;
use std::path::PathBuf;
use std::sync::Arc;

use tracing::debug;

use cache_key::RepositoryUrl;
use dashmap::mapref::one::Ref;
use dashmap::DashMap;
use fs_err::tokio as fs;
use reqwest_middleware::ClientWithMiddleware;
use uv_fs::LockedFile;

use crate::{Fetch, GitReference, GitSha, GitSource, GitUrl, Reporter};

#[derive(Debug, thiserror::Error)]
pub enum GitResolverError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Join(#[from] tokio::task::JoinError),
    #[error("Git operation failed")]
    Git(#[source] anyhow::Error),
}

/// A resolver for Git repositories.
#[derive(Default, Clone)]
pub struct GitResolver(Arc<DashMap<RepositoryReference, GitSha>>);

impl GitResolver {
    /// Inserts a new [`GitSha`] for the given [`RepositoryReference`].
    pub fn insert(&self, reference: RepositoryReference, sha: GitSha) {
        self.0.insert(reference, sha);
    }

    /// Returns the [`GitSha`] for the given [`RepositoryReference`], if it exists.
    fn get(&self, reference: &RepositoryReference) -> Option<Ref<RepositoryReference, GitSha>> {
        self.0.get(reference)
    }

    /// Fetch a remote Git repository.
    pub async fn fetch(
        &self,
        url: &GitUrl,
        client: ClientWithMiddleware,
        cache: PathBuf,
        reporter: Option<impl Reporter + 'static>,
    ) -> Result<Fetch, GitResolverError> {
        debug!("Fetching source distribution from Git: {url}");

        let reference = RepositoryReference::from(url);

        // If we know the precise commit already, reuse it, to ensure that all fetches within a
        // single process are consistent.
        let url = {
            if let Some(precise) = self.get(&reference) {
                Cow::Owned(url.clone().with_precise(*precise))
            } else {
                Cow::Borrowed(url)
            }
        };

        // Avoid races between different processes, too.
        let lock_dir = cache.join("locks");
        fs::create_dir_all(&lock_dir).await?;
        let repository_url = RepositoryUrl::new(url.repository());
        let _lock = LockedFile::acquire(
            lock_dir.join(cache_key::cache_digest(&repository_url)),
            &repository_url,
        )
        .await?;

        // Fetch the Git repository.
        let source = if let Some(reporter) = reporter {
            GitSource::new(url.as_ref().clone(), client, cache).with_reporter(reporter)
        } else {
            GitSource::new(url.as_ref().clone(), client, cache)
        };
        let fetch = tokio::task::spawn_blocking(move || source.fetch())
            .await?
            .map_err(GitResolverError::Git)?;

        // Insert the resolved URL into the in-memory cache. This ensures that subsequent fetches
        // resolve to the same precise commit.
        if let Some(precise) = fetch.git().precise() {
            self.insert(reference, precise);
        }

        Ok(fetch)
    }

    /// Given a remote source distribution, return a precise variant, if possible.
    ///
    /// For example, given a Git dependency with a reference to a branch or tag, return a URL
    /// with a precise reference to the current commit of that branch or tag.
    ///
    /// This method takes into account various normalizations that are independent from the Git
    /// layer. For example: removing `#subdirectory=pkg_dir`-like fragments, and removing `git+`
    /// prefix kinds.
    ///
    /// This method will only return precise URLs for URLs that have already been resolved via
    /// [`resolve_precise`], and will return `None` for URLs that have not been resolved _or_
    /// already have a precise reference.
    pub fn precise(&self, url: GitUrl) -> Option<GitUrl> {
        let reference = RepositoryReference::from(&url);
        let precise = self.get(&reference)?;
        Some(url.with_precise(*precise))
    }

    /// Returns `true` if the two Git URLs refer to the same precise commit.
    pub fn same_ref(&self, a: &GitUrl, b: &GitUrl) -> bool {
        // Convert `a` to a repository URL.
        let a_ref = RepositoryReference::from(a);

        // Convert `b` to a repository URL.
        let b_ref = RepositoryReference::from(b);

        // The URLs must refer to the same repository.
        if a_ref.url != b_ref.url {
            return false;
        }

        // If the URLs have the same tag, they refer to the same commit.
        if a_ref.reference == b_ref.reference {
            return true;
        }

        // Otherwise, the URLs must resolve to the same precise commit.
        let Some(a_precise) = a.precise().or_else(|| self.get(&a_ref).map(|sha| *sha)) else {
            return false;
        };

        let Some(b_precise) = b.precise().or_else(|| self.get(&b_ref).map(|sha| *sha)) else {
            return false;
        };

        a_precise == b_precise
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ResolvedRepositoryReference {
    /// An abstract reference to a Git repository, including the URL and the commit (e.g., a branch,
    /// tag, or revision).
    pub reference: RepositoryReference,
    /// The precise commit SHA of the reference.
    pub sha: GitSha,
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
            url: RepositoryUrl::new(git.repository()),
            reference: git.reference().clone(),
        }
    }
}
