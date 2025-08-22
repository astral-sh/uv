//! Git support is derived from Cargo's implementation.
//! Cargo is dual-licensed under either Apache 2.0 or MIT, at the user's choice.
//! Source: <https://github.com/rust-lang/cargo/blob/23eb492cf920ce051abfc56bbaf838514dc8365c/src/cargo/sources/git/source.rs>

use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use reqwest_middleware::ClientWithMiddleware;
use tracing::{debug, instrument};

use uv_auth::KeyringProvider;
use uv_cache_key::{RepositoryUrl, cache_digest};
use uv_git_types::{GitOid, GitReference, GitUrl};
use uv_redacted::DisplaySafeUrl;

use crate::GIT_STORE;
use crate::git::{GitDatabase, GitRemote};

/// A remote Git source that can be checked out locally.
pub struct GitSource {
    /// The Git reference from the manifest file.
    git: GitUrl,
    /// The HTTP client to use for fetching.
    client: ClientWithMiddleware,
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
    pub fn new(
        git: GitUrl,
        client: impl Into<ClientWithMiddleware>,
        cache: impl Into<PathBuf>,
        offline: bool,
    ) -> Self {
        Self {
            git,
            disable_ssl: false,
            offline,
            client: client.into(),
            cache: cache.into(),
            reporter: None,
        }
    }

    /// Disable SSL verification for this [`GitSource`].
    #[must_use]
    pub fn dangerous(self) -> Self {
        Self {
            disable_ssl: true,
            ..self
        }
    }

    /// Set the [`Reporter`] to use for the [`GitSource`].
    #[must_use]
    pub fn with_reporter(self, reporter: Arc<dyn Reporter>) -> Self {
        Self {
            reporter: Some(reporter),
            ..self
        }
    }

    /// Fetch the underlying Git repository at the given revision.
    #[instrument(skip(self), fields(repository = %self.git.repository(), rev = ?self.git.precise()))]
    pub async fn fetch(self, keyring_provider: Option<&KeyringProvider>) -> Result<Fetch> {
        // Compute the canonical URL for the repository.
        let canonical = RepositoryUrl::new(self.git.repository());

        // The path to the repo, within the Git database.
        let ident = cache_digest(&canonical);
        let db_path = self.cache.join("db").join(&ident);

        let git_store_credentials = GIT_STORE.get(&canonical);
        let credentials = match (&git_store_credentials, keyring_provider) {
            (Some(creds), _) if creds.password().is_some() => git_store_credentials,
            (_, Some(keyring)) => {
                let repo_username = self.git.repository().username();
                let username = if !repo_username.is_empty() {
                    Some(repo_username)
                } else {
                    git_store_credentials
                        .as_ref()
                        .and_then(|creds| creds.username())
                };
                let mut git = self.git.repository().clone();

                git.remove_credentials();

                keyring
                    .fetch_if_native(&git, username)
                    .await
                    .map(Arc::new)
                    .or(git_store_credentials)
            }
            _ => git_store_credentials,
        };

        // Authenticate the URL, if necessary.
        let remote = if let Some(credentials) = credentials {
            Cow::Owned(credentials.apply(self.git.repository().clone()))
        } else {
            Cow::Borrowed(self.git.repository())
        };

        let git = self.git.clone();
        let reporter = self.reporter.clone();
        let remote_clone = remote.as_ref().clone();
        // Fetch the commit, if we don't already have it. Wrapping this section in a closure makes
        // it easier to short-circuit this in the cases where we do have the commit.
        let (db, actual_rev, maybe_task) =
            tokio::task::spawn_blocking(move || -> Result<(GitDatabase, GitOid, Option<usize>)> {
                let remote = remote_clone;
                let git_remote = GitRemote::new(&remote);
                let maybe_db = git_remote.db_at(&db_path).ok();

                // If we have a locked revision, and we have a pre-existing database which has that
                // revision, then no update needs to happen.
                if let (Some(rev), Some(db)) = (git.precise(), &maybe_db) {
                    if db.contains(rev) {
                        debug!("Using existing Git source `{}`", git.repository());
                        return Ok((maybe_db.unwrap(), rev, None));
                    }
                }

                // If the revision isn't locked, but it looks like it might be an exact commit hash,
                // and we do have a pre-existing database, then check whether it is, in fact, a commit
                // hash. If so, treat it like it's locked.
                if let Some(db) = &maybe_db {
                    if let GitReference::BranchOrTagOrCommit(maybe_commit) = git.reference() {
                        if let Ok(oid) = maybe_commit.parse::<GitOid>() {
                            if db.contains(oid) {
                                // This reference is an exact commit. Treat it like it's
                                // locked.
                                debug!("Using existing Git source `{}`", git.repository());
                                return Ok((maybe_db.unwrap(), oid, None));
                            }
                        }
                    }
                }

                // ... otherwise, we use this state to update the Git database. Note that we still check
                // for being offline here, for example in the situation that we have a locked revision
                // but the database doesn't have it.
                debug!("Updating Git source `{}`", git.repository());

                // Report the checkout operation to the reporter.
                let task = reporter.as_ref().map(|reporter| {
                    reporter.on_checkout_start(git_remote.url(), git.reference().as_rev())
                });

                let (db, actual_rev) = git_remote.checkout(
                    &db_path,
                    maybe_db,
                    git.reference(),
                    git.precise(),
                    &self.client,
                    self.disable_ssl,
                    self.offline,
                )?;

                Ok((db, actual_rev, task))
            })
            .await??;

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

        db.copy_to(actual_rev, &checkout_path)?;

        // Report the checkout operation to the reporter.
        if let Some(task) = maybe_task {
            if let Some(reporter) = self.reporter.as_ref() {
                reporter.on_checkout_complete(&remote, actual_rev.as_str(), task);
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
    fn on_checkout_start(&self, url: &DisplaySafeUrl, rev: &str) -> usize;

    /// Callback to invoke when a repository checkout completes.
    fn on_checkout_complete(&self, url: &DisplaySafeUrl, rev: &str, index: usize);
}
