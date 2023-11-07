//! Git support is derived from Cargo's implementation.
//! Cargo is dual-licensed under either Apache 2.0 or MIT, at the user's choice.
//! Source: <https://github.com/rust-lang/cargo/blob/23eb492cf920ce051abfc56bbaf838514dc8365c/src/cargo/sources/git/source.rs>
use std::path::PathBuf;

use anyhow::Result;
use reqwest::Client;
use tracing::debug;
use url::Url;

use puffin_cache::{digest, RepositoryUrl};

use crate::git::GitRemote;
use crate::{FetchStrategy, Git};

/// A remote Git source that can be checked out locally.
pub struct GitSource {
    /// The Git reference from the manifest file.
    git: Git,
    /// The HTTP client to use for fetching.
    client: Client,
    /// The fetch strategy to use when cloning.
    strategy: FetchStrategy,
    /// The path to the Git source database.
    cache: PathBuf,
    /// The reporter to use for this source.
    reporter: Option<Box<dyn Reporter>>,
}

impl GitSource {
    /// Initialize a new Git source.
    pub fn new(git: Git, cache: impl Into<PathBuf>) -> Self {
        Self {
            git,
            client: Client::new(),
            strategy: FetchStrategy::Libgit2,
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
    pub fn fetch(self) -> Result<Fetch> {
        // The path to the repo, within the Git database.
        let ident = digest(&RepositoryUrl::new(&self.git.url));
        let db_path = self.cache.join("db").join(&ident);

        let remote = GitRemote::new(&self.git.url);
        let (db, actual_rev) = match (self.git.precise, remote.db_at(&db_path).ok()) {
            // If we have a locked revision, and we have a preexisting database
            // which has that revision, then no update needs to happen.
            (Some(rev), Some(db)) if db.contains(rev) => (db, rev),

            // ... otherwise we use this state to update the git database. Note
            // that we still check for being offline here, for example in the
            // situation that we have a locked revision but the database
            // doesn't have it.
            (locked_rev, db) => {
                debug!("updating git source `{:?}`", self.git.url);
                if let Some(reporter) = self.reporter.as_ref() {
                    reporter.on_fetch_git_repo(remote.url());
                }

                remote.checkout(
                    &db_path,
                    db,
                    &self.git.reference,
                    locked_rev,
                    self.strategy,
                    &self.client,
                )?
            }
        };

        // Don’t use the full hash, in order to contribute less to reaching the
        // path length limit on Windows.
        let short_id = db.to_short_id(actual_rev)?;

        // Check out `actual_rev` from the database to a scoped location on the
        // filesystem. This will use hard links and such to ideally make the
        // checkout operation here pretty fast.
        let checkout_path = self
            .cache
            .join("checkouts")
            .join(&ident)
            .join(short_id.as_str());
        db.copy_to(actual_rev, &checkout_path, self.strategy, &self.client)?;

        Ok(Fetch {
            git: self.git.with_precise(actual_rev),
            path: checkout_path,
        })
    }
}

pub struct Fetch {
    /// The [`Git`] reference that was fetched.
    git: Git,
    /// The path to the checked out repository.
    path: PathBuf,
}

impl From<Fetch> for Git {
    fn from(fetch: Fetch) -> Self {
        fetch.git
    }
}

impl From<Fetch> for PathBuf {
    fn from(fetch: Fetch) -> Self {
        fetch.path
    }
}

pub trait Reporter: Send + Sync {
    /// Callback to invoke when a repository is checked out.
    fn on_fetch_git_repo(&self, url: &Url);
}
