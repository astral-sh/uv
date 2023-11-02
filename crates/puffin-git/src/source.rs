//! Git support is derived from Cargo's implementation.
//! Cargo is dual-licensed under either Apache 2.0 or MIT, at the user's choice.
//! Source: <https://github.com/rust-lang/cargo/blob/23eb492cf920ce051abfc56bbaf838514dc8365c/src/cargo/sources/git/source.rs>
use std::path::PathBuf;

use anyhow::Result;
use reqwest::Client;
use tracing::debug;

use puffin_cache::{digest, CanonicalUrl};

use crate::git::{GitReference, GitRemote};
use crate::{FetchStrategy, Git};

/// A remote Git source that can be checked out locally.
pub struct GitSource {
    /// The git remote which we're going to fetch from.
    remote: GitRemote,
    /// The Git reference from the manifest file.
    manifest_reference: GitReference,
    /// The revision which a git source is locked to.
    /// This is expected to be set after the Git repository is fetched.
    locked_rev: Option<git2::Oid>,
    /// The identifier of this source for Cargo's Git cache directory.
    /// See [`ident`] for more.
    ident: String,
    /// The HTTP client to use for fetching.
    client: Client,
    /// The fetch strategy to use when cloning.
    strategy: FetchStrategy,
    /// The path to the Git source database.
    git: PathBuf,
}

impl GitSource {
    pub fn new(reference: Git, git: PathBuf) -> Self {
        Self {
            remote: GitRemote::new(&reference.url),
            manifest_reference: reference.reference,
            locked_rev: reference.precise,
            ident: digest(&CanonicalUrl::new(&reference.url)),
            client: Client::new(),
            strategy: FetchStrategy::Libgit2,
            git,
        }
    }

    pub fn fetch(self) -> Result<PathBuf> {
        // The path to the repo, within the Git database.
        let db_path = self.git.join("db").join(&self.ident);

        let (db, actual_rev) = match (self.locked_rev, self.remote.db_at(&db_path).ok()) {
            // If we have a locked revision, and we have a preexisting database
            // which has that revision, then no update needs to happen.
            (Some(rev), Some(db)) if db.contains(rev) => (db, rev),

            // ... otherwise we use this state to update the git database. Note
            // that we still check for being offline here, for example in the
            // situation that we have a locked revision but the database
            // doesn't have it.
            (locked_rev, db) => {
                debug!("Updating Git source: `{:?}`", self.remote);

                self.remote.checkout(
                    &db_path,
                    db,
                    &self.manifest_reference,
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
            .git
            .join("checkouts")
            .join(&self.ident)
            .join(short_id.as_str());
        db.copy_to(actual_rev, &checkout_path, self.strategy, &self.client)?;

        Ok(checkout_path)
    }
}
