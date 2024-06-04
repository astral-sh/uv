use std::path::PathBuf;
use std::sync::Arc;

use tracing::debug;

use cache_key::RepositoryUrl;
use dashmap::mapref::one::Ref;
use dashmap::DashMap;
use fs_err::tokio as fs;
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
    /// Initialize a [`GitResolver`] with a set of resolved references.
    pub fn from_refs(refs: Vec<ResolvedRepositoryReference>) -> Self {
        Self(Arc::new(
            refs.into_iter()
                .map(|ResolvedRepositoryReference { reference, sha }| (reference, sha))
                .collect(),
        ))
    }

    /// Inserts a new [`GitSha`] for the given [`RepositoryReference`].
    pub fn insert(&self, reference: RepositoryReference, sha: GitSha) {
        self.0.insert(reference, sha);
    }

    /// Returns the [`GitSha`] for the given [`RepositoryReference`], if it exists.
    fn get(&self, reference: &RepositoryReference) -> Option<Ref<RepositoryReference, GitSha>> {
        self.0.get(reference)
    }

    /// Download a source distribution from a Git repository.
    ///
    /// Assumes that the URL is a precise Git URL, with a full commit hash.
    pub async fn fetch(
        &self,
        url: &GitUrl,
        cache: PathBuf,
        reporter: Option<impl Reporter + 'static>,
    ) -> Result<Fetch, GitResolverError> {
        debug!("Fetching source distribution from Git: {url}");

        // Avoid races between different processes, too.
        let lock_dir = cache.join("locks");
        fs::create_dir_all(&lock_dir).await?;
        let repository_url = RepositoryUrl::new(url.repository());
        let _lock = LockedFile::acquire(
            lock_dir.join(cache_key::digest(&repository_url)),
            &repository_url,
        )?;

        // Fetch the Git repository.
        let source = if let Some(reporter) = reporter {
            GitSource::new(url.clone(), cache).with_reporter(reporter)
        } else {
            GitSource::new(url.clone(), cache)
        };
        let fetch = tokio::task::spawn_blocking(move || source.fetch())
            .await?
            .map_err(GitResolverError::Git)?;

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
    pub async fn resolve(
        &self,
        url: &GitUrl,
        cache: impl Into<PathBuf>,
        reporter: Option<impl Reporter + 'static>,
    ) -> Result<Option<GitUrl>, GitResolverError> {
        // If the Git reference already contains a complete SHA, short-circuit.
        if url.precise().is_some() {
            return Ok(None);
        }

        // If the Git reference is in the in-memory cache, return it.
        {
            let reference = RepositoryReference::from(url);
            if let Some(precise) = self.get(&reference) {
                return Ok(Some(url.clone().with_precise(*precise)));
            }
        }

        // Fetch the precise SHA of the Git reference (which could be a branch, a tag, a partial
        // commit, etc.).
        let source = if let Some(reporter) = reporter {
            GitSource::new(url.clone(), cache).with_reporter(reporter)
        } else {
            GitSource::new(url.clone(), cache)
        };
        let fetch = tokio::task::spawn_blocking(move || source.fetch())
            .await?
            .map_err(GitResolverError::Git)?;
        let git = fetch.into_git();

        // Insert the resolved URL into the in-memory cache.
        if let Some(precise) = git.precise() {
            let reference = RepositoryReference::from(url);
            self.insert(reference, precise);
        }

        Ok(Some(git))
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
    /// [`resolve_precise`].
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
