//! Git support is derived from Cargo's implementation.
//! Cargo is dual-licensed under either Apache 2.0 or MIT, at the user's choice.
//! Source: <https://github.com/rust-lang/cargo/blob/23eb492cf920ce051abfc56bbaf838514dc8365c/src/cargo/sources/git/source.rs>

use std::borrow::Cow;
use std::path::{Path, PathBuf};

use anyhow::Result;
use reqwest_middleware::ClientWithMiddleware;
use tracing::{debug, instrument};
use url::Url;

use cache_key::{cache_digest, RepositoryUrl};

use crate::git::GitRemote;
use crate::{GitOid, GitSha, GitUrl, GIT_STORE};

/// A remote Git source that can be checked out locally.
pub struct GitSource {
    /// The Git reference from the manifest file.
    git: GitUrl,
    /// The HTTP client to use for fetching.
    client: ClientWithMiddleware,
    /// The path to the Git source database.
    cache: PathBuf,
    /// The reporter to use for this source.
    reporter: Option<Box<dyn Reporter>>,
}

impl GitSource {
    /// Initialize a new Git source.
    pub fn new(
        git: GitUrl,
        client: impl Into<ClientWithMiddleware>,
        cache: impl Into<PathBuf>,
    ) -> Self {
        Self {
            git,
            client: client.into(),
            cache: cache.into(),
            reporter: None,
        }
    }

    /// Set the [`Reporter`] to use for this `GIt` source.
    #[must_use]
    pub fn with_reporter(self, reporter: impl Reporter + 'static) -> Self {
        Self {
            reporter: Some(Box::new(reporter)),
            ..self
        }
    }

    /// Fetch the underlying Git repository at the given revision.
    #[instrument(skip(self), fields(repository = %self.git.repository, rev = ?self.git.precise))]
    pub fn fetch(self) -> Result<Fetch> {
        // Compute the canonical URL for the repository.
        let canonical = RepositoryUrl::new(&self.git.repository);

        // The path to the repo, within the Git database.
        let ident = cache_digest(&canonical);
        let db_path = self.cache.join("db").join(&ident);

        // Authenticate the URL, if necessary.
        let remote = if let Some(credentials) = GIT_STORE.get(&canonical) {
            Cow::Owned(credentials.apply(self.git.repository.clone()))
        } else {
            Cow::Borrowed(&self.git.repository)
        };

        let remote = GitRemote::new(&remote);
        let (db, actual_rev, task) = match (self.git.precise, remote.db_at(&db_path).ok()) {
            // If we have a locked revision, and we have a preexisting database
            // which has that revision, then no update needs to happen.
            (Some(rev), Some(db)) if db.contains(rev.into()) => {
                debug!("Using existing Git source `{}`", self.git.repository);
                (db, rev, None)
            }

            // ... otherwise we use this state to update the git database. Note
            // that we still check for being offline here, for example in the
            // situation that we have a locked revision but the database
            // doesn't have it.
            (locked_rev, db) => {
                debug!("Updating Git source `{}`", self.git.repository);

                // Report the checkout operation to the reporter.
                let task = self.reporter.as_ref().map(|reporter| {
                    reporter.on_checkout_start(remote.url(), self.git.reference.as_rev())
                });

                let (db, actual_rev) = remote.checkout(
                    &db_path,
                    db,
                    &self.git.reference,
                    locked_rev.map(GitOid::from),
                    &self.client,
                )?;

                (db, GitSha::from(actual_rev), task)
            }
        };

        // Don’t use the full hash, in order to contribute less to reaching the
        // path length limit on Windows.
        let short_id = db.to_short_id(actual_rev.into())?;

        // Check out `actual_rev` from the database to a scoped location on the
        // filesystem. This will use hard links and such to ideally make the
        // checkout operation here pretty fast.
        let checkout_path = self
            .cache
            .join("checkouts")
            .join(&ident)
            .join(short_id.as_str());

        db.copy_to(actual_rev.into(), &checkout_path)?;

        // Report the checkout operation to the reporter.
        if let Some(task) = task {
            if let Some(reporter) = self.reporter.as_ref() {
                reporter.on_checkout_complete(remote.url(), short_id.as_str(), task);
            }
        }

        Ok(Fetch {
            git: self.git.with_precise(actual_rev),
            path: checkout_path,
        })
    }
}

pub struct Fetch {
    /// The [`GitUrl`] reference that was fetched.
    git: GitUrl,
    /// The path to the checked out repository.
    path: PathBuf,
}

impl Fetch {
    pub fn git(&self) -> &GitUrl {
        &self.git
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn into_git(self) -> GitUrl {
        self.git
    }

    pub fn into_path(self) -> PathBuf {
        self.path
    }
}

pub trait Reporter: Send + Sync {
    /// Callback to invoke when a repository checkout begins.
    fn on_checkout_start(&self, url: &Url, rev: &str) -> usize;

    /// Callback to invoke when a repository checkout completes.
    fn on_checkout_complete(&self, url: &Url, rev: &str, index: usize);
}
