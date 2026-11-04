//! Git support is derived from Cargo's implementation.
//! Cargo is dual-licensed under either Apache 2.0 or MIT, at the user's choice.
//! Source: <https://github.com/rust-lang/cargo/blob/23eb492cf920ce051abfc56bbaf838514dc8365c/src/cargo/sources/git/source.rs>

use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use tracing::{debug, instrument};

use uv_cache_key::cache_digest;
use uv_git_types::{GitOid, GitUrl};
use uv_redacted::SafeUrl;

use crate::credentials::GIT_STORE;
use crate::git::{GitCheckout, GitDatabase, GitRemote};

/// A remote Git source that can be checked out locally.
pub(crate) struct GitSource {
    /// The Git reference from the manifest file.
    git: GitUrl,
    /// Whether to disable SSL verification.
    disable_ssl: bool,
    /// Whether to operate without network connectivity.
    offline: bool,
    /// The path to the Git source database.
    cache: PathBuf,
    /// The reporter to use for this source.
    reporter: Option<Arc<dyn Reporter>>,
}

impl GitSource {
    /// Initialize a [`GitSource`] with the given Git URL, HTTP client, and cache path.
    pub(crate) fn new(git: GitUrl, cache: impl Into<PathBuf>, offline: bool) -> Self {
        Self {
            git,
            disable_ssl: false,
            offline,
            cache: cache.into(),
            reporter: None,
        }
    }

    /// Disable SSL verification for this [`GitSource`].
    #[must_use]
    pub(crate) fn dangerous(self) -> Self {
        Self {
            disable_ssl: true,
            ..self
        }
    }

    /// Set the [`Reporter`] to use for the [`GitSource`].
    #[must_use]
    pub(crate) fn with_reporter(self, reporter: Arc<dyn Reporter>) -> Self {
        Self {
            reporter: Some(reporter),
            ..self
        }
    }

    /// Fetch the underlying Git repository at the given revision.
    #[instrument(skip(self), fields(repository = %self.git.url(), rev = ?self.git.precise()))]
    pub(crate) fn fetch(self) -> Result<Fetch> {
        let lfs_requested = self.git.lfs().enabled();

        // The path to the repo, within the Git database.
        let ident = cache_digest(self.git.repository());
        let db_path = self.cache.join("db").join(&ident);

        // Authenticate the URL, if necessary.
        let remote = if let Some(credentials) = GIT_STORE.get(self.git.repository()) {
            Cow::Owned(credentials.apply(self.git.url().clone()))
        } else {
            Cow::Borrowed(self.git.url())
        };

        // Cache LFS and plain Git checkouts separately because their worktrees can differ.
        let checkout_ident = if lfs_requested {
            cache_digest(&self.git.repository().clone().with_lfs(Some(true)))
        } else {
            ident
        };
        let checkouts = self.cache.join("checkouts").join(checkout_ident);
        let copy_to = |db: GitDatabase, rev: GitOid, path: PathBuf| -> Result<Fetch> {
            let git = self.git.clone().with_precise(rev)?;
            let checkout = db.copy_to(rev, &path)?;
            Ok(Fetch {
                git,
                path,
                lfs_ready: checkout.lfs_ready().unwrap_or(false),
            })
        };

        let git_remote = GitRemote::new(remote.clone().into_owned());
        let maybe_db = match (self.git.precise(), git_remote.db_at(&db_path).ok()) {
            (Some(rev), Some(db)) if db.contains(rev) => {
                let path = checkouts.join(db.to_short_id(rev)?);
                // A completed checkout can serve this revision without reading the shared LFS store.
                if lfs_requested && GitCheckout::open_lfs_ready(rev, &path).is_some() {
                    return Ok(Fetch {
                        git: self.git.clone().with_precise(rev)?,
                        path,
                        lfs_ready: true,
                    });
                }
                if !lfs_requested || db.contains_lfs_artifacts(rev) {
                    debug!("Using existing Git source `{}`", self.git.url());
                    return copy_to(db.with_lfs_ready(lfs_requested.then_some(true)), rev, path);
                }
                Some(db)
            }
            (_, db) => db,
        };

        debug!("Updating Git source `{}`", self.git.url());
        let task = self.reporter.as_ref().map(|reporter| {
            reporter.on_checkout_start(git_remote.url(), self.git.reference().as_rev())
        });
        let (db, rev) = git_remote.checkout(
            &db_path,
            maybe_db,
            self.git.reference(),
            self.git.precise(),
            self.disable_ssl,
            self.offline,
            lfs_requested,
        )?;
        // Git's abbreviated revision keeps checkout paths short on Windows.
        let path = checkouts.join(db.to_short_id(rev)?);
        let fetch = copy_to(db, rev, path)?;

        if let Some(task) = task
            && let Some(reporter) = self.reporter.as_ref()
        {
            reporter.on_checkout_complete(remote.as_ref(), rev.as_str(), task);
        }
        Ok(fetch)
    }
}

pub struct Fetch {
    /// The [`GitUrl`] reference that was fetched.
    git: GitUrl,
    /// The path to the checked out repository.
    path: PathBuf,
    /// Git LFS artifacts have been initialized (if requested).
    lfs_ready: bool,
}

impl Fetch {
    pub fn git(&self) -> &GitUrl {
        &self.git
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn lfs_ready(&self) -> &bool {
        &self.lfs_ready
    }
}

pub trait Reporter: Send + Sync {
    /// Callback to invoke when a repository checkout begins.
    fn on_checkout_start(&self, url: &SafeUrl, rev: &str) -> usize;

    /// Callback to invoke when a repository checkout completes.
    fn on_checkout_complete(&self, url: &SafeUrl, rev: &str, index: usize);
}
