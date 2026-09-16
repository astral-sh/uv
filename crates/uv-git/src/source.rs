//! Git support is derived from Cargo's implementation.
//! Cargo is dual-licensed under either Apache 2.0 or MIT, at the user's choice.
//! Source: <https://github.com/rust-lang/cargo/blob/23eb492cf920ce051abfc56bbaf838514dc8365c/src/cargo/sources/git/source.rs>

use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use tracing::{debug, instrument};

use uv_cache_key::cache_digest;
use uv_git_types::{GitOid, GitReference, GitUrl};
use uv_redacted::DisplaySafeUrl;

use crate::credentials::GIT_STORE;
use crate::git::{GitDatabase, GitRemote};

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

    /// Authenticate the Git URL, if credentials were supplied for the repository.
    fn authenticated_remote(&self) -> Cow<'_, DisplaySafeUrl> {
        if let Some(credentials) = GIT_STORE.get(self.git.repository()) {
            Cow::Owned(credentials.apply(self.git.url().clone()))
        } else {
            Cow::Borrowed(self.git.url())
        }
    }

    /// Fetch the requested revision into the repository database, without checking out its tree.
    fn database(&self, lfs_requested: bool) -> Result<(GitDatabase, GitOid, Option<usize>)> {
        let db_path = self
            .cache
            .join("db")
            .join(cache_digest(self.git.repository()));
        let git_remote = GitRemote::new(self.authenticated_remote().into_owned());

        // A cached, locked revision needs no update. When requested, its LFS artifacts must also
        // have been fetched and validated.
        let database = if let Ok(db) = git_remote.db_at(&db_path) {
            if let Some(revision) = self.git.precise()
                && db.contains(revision)
                && (!lfs_requested || db.contains_lfs_artifacts(revision))
            {
                debug!("Using existing Git source `{}`", self.git.url());
                return Ok((
                    db.with_lfs_ready(lfs_requested.then_some(true)),
                    revision,
                    None,
                ));
            }

            // Treat an exact commit hash as locked if it is already in the database.
            if let GitReference::BranchOrTagOrCommit(reference) = self.git.reference()
                && let Ok(revision) = reference.parse::<GitOid>()
                && db.contains(revision)
                && (!lfs_requested || db.contains_lfs_artifacts(revision))
            {
                debug!("Using existing Git source `{}`", self.git.url());
                return Ok((
                    db.with_lfs_ready(lfs_requested.then_some(true)),
                    revision,
                    None,
                ));
            }

            Some(db)
        } else {
            None
        };
        debug!("Updating Git source `{}`", self.git.url());

        let task = self.reporter.as_ref().map(|reporter| {
            reporter.on_checkout_start(git_remote.url(), self.git.reference().as_rev())
        });
        let (database, revision) = git_remote.checkout(
            &db_path,
            database,
            self.git.reference(),
            self.git.precise(),
            self.disable_ssl,
            self.offline,
            lfs_requested,
        )?;
        Ok((database, revision, task))
    }

    /// Resolve a revision without checking out files or fetching submodules or Git LFS.
    #[instrument(skip(self), fields(repository = %self.git.url(), rev = ?self.git.precise()))]
    pub(crate) fn resolve_reference(self) -> Result<GitOid> {
        let (_, revision, _) = self.database(false)?;
        self.git.with_precise(revision)?;
        Ok(revision)
    }

    /// Fetch the underlying Git repository at the given revision.
    #[instrument(skip(self), fields(repository = %self.git.url(), rev = ?self.git.precise()))]
    pub(crate) fn fetch(self) -> Result<Fetch> {
        let lfs_requested = self.git.lfs().enabled();
        let (db, actual_rev, maybe_task) = self.database(lfs_requested)?;

        // Validate the resolved commit before checking out its contents.
        let git = self.git.clone().with_precise(actual_rev)?;

        // Don’t use the full hash, in order to contribute less to reaching the
        // path length limit on Windows.
        let short_id = db.to_short_id(actual_rev)?;

        // Compute the canonical URL for the repository checkout.
        let canonical = git.repository().clone().with_lfs(Some(lfs_requested));
        // Recompute the checkout hash when Git LFS is enabled as we want
        // to distinctly differentiate between LFS vs non-LFS source trees.
        let ident = if lfs_requested {
            cache_digest(&canonical)
        } else {
            cache_digest(git.repository())
        };
        let checkout_path = self
            .cache
            .join("checkouts")
            .join(&ident)
            .join(short_id.as_str());

        // Check out `actual_rev` from the database to a scoped location on the
        // filesystem. This will use hard links and such to ideally make the
        // checkout operation here pretty fast.
        let checkout = db.copy_to(actual_rev, &checkout_path)?;

        // Report the checkout operation to the reporter.
        if let Some(task) = maybe_task {
            if let Some(reporter) = self.reporter.as_ref() {
                reporter.on_checkout_complete(
                    self.authenticated_remote().as_ref(),
                    actual_rev.as_str(),
                    task,
                );
            }
        }

        Ok(Fetch {
            git,
            path: checkout_path,
            lfs_ready: checkout.lfs_ready().unwrap_or(false),
        })
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
    fn on_checkout_start(&self, url: &DisplaySafeUrl, rev: &str) -> usize;

    /// Callback to invoke when a repository checkout completes.
    fn on_checkout_complete(&self, url: &DisplaySafeUrl, rev: &str, index: usize);
}
