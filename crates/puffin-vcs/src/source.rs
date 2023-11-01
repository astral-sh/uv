use std::path::PathBuf;

use tracing::debug;

use crate::config::Config;
use crate::util::{short_hash, CanonicalUrl, CargoResult};
use crate::{GitDependencyReference, GitReference, GitRemote};

pub struct GitSource {
    /// The configuration for Cargo.
    config: Config,
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
    /// The path to the Git source database.
    git: PathBuf,
    /// The path to which the Git source has been checked out.
    source: Option<PathBuf>,
}

impl GitSource {
    pub fn new(reference: GitDependencyReference, git: PathBuf) -> CargoResult<Self> {
        Ok(Self {
            config: Config::new(),
            remote: GitRemote::new(&reference.url),
            manifest_reference: reference.reference,
            locked_rev: reference.precise,
            ident: short_hash(&CanonicalUrl::new(&reference.url)?),
            git,
            source: None,
        })
    }

    pub fn fetch(self) -> CargoResult<PathBuf> {
        let source = self.git.join("db").join(&self.ident);

        let db = self.remote.db_at(&self.git).ok();
        let (db, actual_rev) = match (self.locked_rev, db) {
            // If we have a locked revision, and we have a preexisting database
            // which has that revision, then no update needs to happen.
            (Some(rev), Some(db)) if db.contains(rev) => (db, rev),

            // ... otherwise we use this state to update the git database. Note
            // that we still check for being offline here, for example in the
            // situation that we have a locked revision but the database
            // doesn't have it.
            (locked_rev, db) => {
                debug!("Updating git source: `{:?}`", self.remote);

                self.remote.checkout(
                    &source,
                    db,
                    &self.manifest_reference,
                    locked_rev,
                    &self.config,
                )?
            }
        };

        // Don’t use the full hash, in order to contribute less to reaching the
        // path length limit on Windows. See
        // <https://github.com/servo/servo/pull/14397>.
        let short_id = db.to_short_id(actual_rev)?;

        // Check out `actual_rev` from the database to a scoped location on the
        // filesystem. This will use hard links and such to ideally make the
        // checkout operation here pretty fast.
        let checkout_path = self
            .git
            .join("checkouts")
            .join(&self.ident)
            .join(short_id.as_str());
        db.copy_to(actual_rev, &checkout_path, &self.config)?;

        Ok(checkout_path)
    }
}
