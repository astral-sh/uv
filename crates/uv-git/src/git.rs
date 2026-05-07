//! Git support is derived from Cargo's implementation.
//! Cargo is dual-licensed under either Apache 2.0 or MIT, at the user's choice.
//! Source: <https://github.com/rust-lang/cargo/blob/23eb492cf920ce051abfc56bbaf838514dc8365c/src/cargo/sources/git/utils.rs>
use std::env;
use std::fmt::Display;
use std::path::{Path, PathBuf};
use std::str::{self};
use std::sync::LazyLock;

use anyhow::{Context, Result, anyhow};
use cargo_util::{ProcessBuilder, paths};
use owo_colors::OwoColorize;
use tracing::{debug, instrument, warn};

use uv_fs::Simplified;
use uv_git_types::{GitOid, GitReference};
use uv_redacted::DisplaySafeUrl;
use uv_static::EnvVars;
use uv_warnings::warn_user_once;

/// A file indicates that if present, `git reset` has been done and a repo
/// checkout is ready to go. See [`GitCheckout::reset`] for why we need this.
const CHECKOUT_READY_LOCK: &str = ".ok";

/// Filter used for partial Git fetches.
const PARTIAL_CLONE_FILTER: &str = "tree:0";

/// Remote configured for local checkout clones.
const CHECKOUT_REMOTE: &str = "origin";

#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error("Git executable not found. Ensure that Git is installed and available.")]
    GitNotFound,
    #[error("Git LFS extension not found. Ensure that Git LFS is installed and available.")]
    GitLfsNotFound,
    #[error("Is Git LFS configured? Run `{}` to initialize Git LFS.", "git lfs install".green())]
    GitLfsNotConfigured,
    #[error(transparent)]
    Other(#[from] which::Error),
    #[error(
        "Remote Git fetches are not allowed because network connectivity is disabled (i.e., with `--offline`)"
    )]
    TransportNotAllowed,
}

/// A global cache of the result of `which git` as a command
///
/// Caching the command allows us to avoid needing to remove environment
/// variables everywhere.
pub static GIT: LazyLock<Result<ProcessBuilder, GitError>> = LazyLock::new(|| {
    let path = which::which("git").map_err(|err| match err {
        which::Error::CannotFindBinaryPath => GitError::GitNotFound,
        err => GitError::Other(err),
    })?;

    let mut cmd = ProcessBuilder::new(path);

    // Certain git environment variables never make sense to inherit because
    // they affect what the current command will act on.

    // This can cause problems if for example uv is ran by git (for example, the
    // `exec` command in `git rebase`), the GIT_DIR is set by git and will point
    // to the wrong location (this takes precedence over the cwd).
    cmd.env_remove(EnvVars::GIT_DIR)
        .env_remove(EnvVars::GIT_WORK_TREE)
        .env_remove(EnvVars::GIT_INDEX_FILE)
        .env_remove(EnvVars::GIT_OBJECT_DIRECTORY)
        .env_remove(EnvVars::GIT_ALTERNATE_OBJECT_DIRECTORIES)
        .env_remove(EnvVars::GIT_COMMON_DIR);

    Ok(cmd)
});

/// Strategy when fetching refspecs for a [`GitReference`]
enum RefspecStrategy {
    /// All refspecs should be fetched, if any fail then the fetch will fail.
    All,
    /// Stop after the first successful fetch, if none succeed then the fetch will fail.
    First,
}

/// A Git reference (like a tag or branch) or a specific commit.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum ReferenceOrOid<'reference> {
    /// A Git reference, like a tag or branch.
    Reference(&'reference GitReference),
    /// A specific commit.
    Oid(GitOid),
}

impl ReferenceOrOid<'_> {
    /// Resolves the [`ReferenceOrOid`] to an object ID with objects the `repo` currently has.
    fn resolve(&self, repo: &GitRepository) -> Result<GitOid> {
        let refkind = self.kind_str();
        let result = match self {
            // Resolve the commit pointed to by the tag.
            //
            // `^0` recursively peels away from the revision to the underlying commit object.
            // This also verifies that the tag indeed refers to a commit.
            Self::Reference(GitReference::Tag(s)) => {
                repo.rev_parse(&format!("refs/remotes/origin/tags/{s}^0"))
            }

            // Resolve the commit pointed to by the branch.
            Self::Reference(GitReference::Branch(s)) => repo.rev_parse(&format!("origin/{s}^0")),

            // Attempt to resolve the branch, then the tag.
            Self::Reference(GitReference::BranchOrTag(s)) => repo
                .rev_parse(&format!("origin/{s}^0"))
                .or_else(|_| repo.rev_parse(&format!("refs/remotes/origin/tags/{s}^0"))),

            // Attempt to resolve the branch, then the tag, then the commit.
            Self::Reference(GitReference::BranchOrTagOrCommit(s)) => repo
                .rev_parse(&format!("origin/{s}^0"))
                .or_else(|_| repo.rev_parse(&format!("refs/remotes/origin/tags/{s}^0")))
                .or_else(|_| repo.rev_parse(&format!("{s}^0"))),

            // We'll be using the HEAD commit.
            Self::Reference(GitReference::DefaultBranch) => {
                repo.rev_parse("refs/remotes/origin/HEAD")
            }

            // Resolve a named reference.
            Self::Reference(GitReference::NamedRef(s)) => repo.rev_parse(&format!("{s}^0")),

            // Resolve a specific commit.
            Self::Oid(s) => repo.rev_parse(&format!("{s}^0")),
        };

        result.with_context(|| anyhow::format_err!("failed to find {refkind} `{self}`"))
    }

    /// Returns the kind of this [`ReferenceOrOid`].
    fn kind_str(&self) -> &str {
        match self {
            Self::Reference(reference) => reference.kind_str(),
            Self::Oid(_) => "commit",
        }
    }

    /// Converts the [`ReferenceOrOid`] to a `str` that can be used as a revision.
    fn as_rev(&self) -> &str {
        match self {
            Self::Reference(r) => r.as_rev(),
            Self::Oid(rev) => rev.as_str(),
        }
    }
}

impl Display for ReferenceOrOid<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Reference(reference) => write!(f, "{reference}"),
            Self::Oid(oid) => write!(f, "{oid}"),
        }
    }
}

/// A remote repository. It gets cloned into a local [`GitDatabase`].
#[derive(PartialEq, Clone, Debug)]
pub(crate) struct GitRemote {
    /// URL to a remote repository.
    url: DisplaySafeUrl,
}

/// A local clone of a remote repository's database. Multiple [`GitCheckout`]s
/// can be cloned from a single [`GitDatabase`].
pub(crate) struct GitDatabase {
    /// Underlying Git repository instance for this database.
    repo: GitRepository,
    /// The upstream promisor remote for partial clones, if any.
    promisor_remote: Option<DisplaySafeUrl>,
    /// Git LFS artifacts have been initialized (if requested).
    lfs_ready: Option<bool>,
}

/// A local checkout of a particular revision from a [`GitRepository`].
pub(crate) struct GitCheckout {
    /// The git revision this checkout is for.
    revision: GitOid,
    /// Underlying Git repository instance for this checkout.
    repo: GitRepository,
    /// Git LFS artifacts have been initialized (if requested).
    lfs_ready: Option<bool>,
}

/// A local Git repository.
pub(crate) struct GitRepository {
    /// Path to the underlying Git repository on the local filesystem.
    path: PathBuf,
}

impl GitRepository {
    /// Opens an existing Git repository at `path`.
    pub(crate) fn open(path: &Path) -> Result<Self> {
        // Make sure there is a Git repository at the specified path.
        GIT.as_ref()
            .cloned()?
            .arg("rev-parse")
            .cwd(path)
            .exec_with_output()?;

        Ok(Self {
            path: path.to_path_buf(),
        })
    }

    /// Initializes a Git repository at `path`.
    fn init(path: &Path) -> Result<Self> {
        // TODO(ibraheem): see if this still necessary now that we no longer use libgit2
        // Skip anything related to templates, they just call all sorts of issues as
        // we really don't want to use them yet they insist on being used. See #6240
        // for an example issue that comes up.
        // opts.external_template(false);

        // Initialize the repository.
        GIT.as_ref()
            .cloned()?
            .arg("init")
            .cwd(path)
            .exec_with_output()?;

        Ok(Self {
            path: path.to_path_buf(),
        })
    }

    /// Returns the configured Git remotes for this repository.
    fn remotes(&self) -> Result<Vec<String>> {
        let output = GIT
            .as_ref()
            .cloned()?
            .arg("remote")
            .cwd(&self.path)
            .exec_with_output()?;

        let output = String::from_utf8(output.stdout)?;
        Ok(output
            .lines()
            .map(str::trim)
            .filter(|remote| !remote.is_empty())
            .map(ToString::to_string)
            .collect())
    }

    /// Returns the upstream promisor remote for this repository, if any.
    ///
    /// Partial clones fetched directly from a URL are recorded as a remote whose
    /// name is that URL, so `git remote` is sufficient to recover the original
    /// upstream without depending on the checkout clone's `origin` remote.
    fn promisor_remote(&self) -> Result<Option<DisplaySafeUrl>> {
        Ok(self
            .remotes()?
            .into_iter()
            .find_map(|remote| remote.parse().ok()))
    }

    /// Configures the given remote as the promisor remote for this repository.
    fn configure_promisor_remote(&self, remote: &str, url: &DisplaySafeUrl) -> Result<()> {
        let url = without_credentials(url);
        let remotes = self.remotes()?;

        if remotes.iter().any(|existing| existing == remote) {
            GIT.as_ref()
                .cloned()?
                .arg("remote")
                .arg("set-url")
                .arg(remote)
                .arg(url.as_str())
                .cwd(&self.path)
                .exec_with_output()?;
        } else {
            GIT.as_ref()
                .cloned()?
                .arg("remote")
                .arg("add")
                .arg(remote)
                .arg(url.as_str())
                .cwd(&self.path)
                .exec_with_output()?;
        }

        GIT.as_ref()
            .cloned()?
            .arg("config")
            .arg(format!("remote.{remote}.promisor"))
            .arg("true")
            .cwd(&self.path)
            .exec_with_output()?;

        GIT.as_ref()
            .cloned()?
            .arg("config")
            .arg(format!("remote.{remote}.partialclonefilter"))
            .arg(PARTIAL_CLONE_FILTER)
            .cwd(&self.path)
            .exec_with_output()?;

        // Git creates a promisor remote whose name is the fetch URL when fetching
        // with `--filter` from a URL. Remove any such URL-named remotes so we
        // don't persist credentials in `.git/config`.
        for existing in remotes {
            if existing == remote {
                continue;
            }
            let Ok(existing_url) = existing.parse::<DisplaySafeUrl>() else {
                continue;
            };
            if without_credentials(&existing_url) == url {
                let result = GIT
                    .as_ref()
                    .cloned()?
                    .arg("config")
                    .arg("--remove-section")
                    .arg(format!("remote.{existing}"))
                    .cwd(&self.path)
                    .exec_with_output();
                if let Err(err) = result {
                    debug!("Failed to remove URL-named Git remote `{existing}`: {err}");
                }
            }
        }

        Ok(())
    }

    /// Parses the object ID of the given `refname`.
    fn rev_parse(&self, refname: &str) -> Result<GitOid> {
        let result = GIT
            .as_ref()
            .cloned()?
            .arg("rev-parse")
            .arg(refname)
            // Avoid triggering dynamic object fetches when we are only checking
            // whether a revision resolves locally.
            .env("GIT_NO_LAZY_FETCH", "1")
            .cwd(&self.path)
            .exec_with_output()?;

        let mut result = String::from_utf8(result.stdout)?;
        result.truncate(result.trim_end().len());
        Ok(result.parse()?)
    }

    /// Verifies LFS artifacts have been initialized for a given `refname`.
    #[instrument(skip_all, fields(path = %self.path.user_display(), refname = %refname))]
    fn lfs_fsck_objects(&self, refname: &str) -> bool {
        let mut cmd = if let Ok(lfs) = GIT_LFS.as_ref() {
            lfs.clone()
        } else {
            warn!("Git LFS is not available, skipping LFS fetch");
            return false;
        };

        // Requires Git LFS 3.x (2021 release)
        let result = cmd
            .arg("fsck")
            .arg("--objects")
            .arg(refname)
            .cwd(&self.path)
            .exec_with_output();

        match result {
            Ok(_) => true,
            Err(err) => {
                let lfs_error = err.to_string();
                if lfs_error.contains("unknown flag: --objects") {
                    warn_user_once!(
                        "Skipping Git LFS validation as Git LFS extension is outdated. \
                        Upgrade to `git-lfs>=3.0.2` or manually verify git-lfs objects were \
                        properly fetched after the current operation finishes."
                    );
                    true
                } else {
                    debug!("Git LFS validation failed: {err}");
                    false
                }
            }
        }
    }
}

impl GitRemote {
    /// Creates an instance for a remote repository URL.
    pub(crate) fn new(url: &DisplaySafeUrl) -> Self {
        Self { url: url.clone() }
    }

    /// Gets the remote repository URL.
    pub(crate) fn url(&self) -> &DisplaySafeUrl {
        &self.url
    }

    /// Fetches and checkouts to a reference or a revision from this remote
    /// into a local path.
    ///
    /// This ensures that it gets the up-to-date commit when a named reference
    /// is given (tag, branch, refs/*). Thus, network connection is involved.
    ///
    /// When `locked_rev` is provided, it takes precedence over `reference`.
    ///
    /// If we have a previous instance of [`GitDatabase`] then fetch into that
    /// if we can. If that can successfully load our revision then we've
    /// populated the database with the latest version of `reference`, so
    /// return that database and the rev we resolve to.
    pub(crate) fn checkout(
        &self,
        into: &Path,
        db: Option<GitDatabase>,
        reference: &GitReference,
        locked_rev: Option<GitOid>,
        disable_ssl: bool,
        offline: bool,
        with_lfs: bool,
    ) -> Result<(GitDatabase, GitOid)> {
        let reference = locked_rev
            .map(ReferenceOrOid::Oid)
            .unwrap_or(ReferenceOrOid::Reference(reference));
        if let Some(mut db) = db {
            fetch(&mut db.repo, &self.url, reference, disable_ssl, offline)
                .with_context(|| format!("failed to fetch into: {}", into.user_display()))?;

            let resolved_commit_hash = match locked_rev {
                Some(rev) => db.contains(rev).then_some(rev),
                None => reference.resolve(&db.repo).ok(),
            };

            if let Some(rev) = resolved_commit_hash {
                db = db.with_lfs_ready(with_lfs.then_some(true));
                return Ok((db, rev));
            }
        }

        // Otherwise start from scratch to handle corrupt git repositories.
        // After our fetch (which is interpreted as a clone now) we do the same
        // resolution to figure out what we cloned.
        match fs_err::remove_dir_all(into) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }

        fs_err::create_dir_all(into)?;
        let mut repo = GitRepository::init(into)?;
        fetch(&mut repo, &self.url, reference, disable_ssl, offline)
            .with_context(|| format!("failed to clone into: {}", into.user_display()))?;
        let rev = match locked_rev {
            Some(rev) => rev,
            None => reference.resolve(&repo)?,
        };

        Ok((
            GitDatabase {
                repo,
                promisor_remote: Some(self.url.clone()),
                lfs_ready: with_lfs.then_some(true),
            },
            rev,
        ))
    }

    /// Creates a [`GitDatabase`] of this remote at `db_path`.
    pub(crate) fn db_at(&self, db_path: &Path) -> Result<GitDatabase> {
        let repo = GitRepository::open(db_path)?;
        let has_promisor_remote = repo.promisor_remote()?.is_some();
        let promisor_remote = Some(self.url.clone());

        if has_promisor_remote {
            repo.configure_promisor_remote(CHECKOUT_REMOTE, &self.url)?;
        } else {
            // Older uv versions populated the shared Git database without recording
            // promisor metadata. Keep using the current remote URL as a fallback so
            // checkout clones can still recover missing trees and Git LFS objects
            // from those legacy caches.
            debug!("Using current Git URL as promisor remote fallback for legacy database clone");
        }

        Ok(GitDatabase {
            repo,
            promisor_remote,
            lfs_ready: None,
        })
    }
}

impl GitDatabase {
    /// Checkouts to a revision at `destination` from this database.
    pub(crate) fn copy_to(
        &self,
        rev: GitOid,
        destination: &Path,
        disable_ssl: bool,
        offline: bool,
    ) -> Result<GitCheckout> {
        // If the existing checkout exists, and it is fresh, use it.
        // A non-fresh checkout can happen if the checkout operation was
        // interrupted. In that case, the checkout gets deleted and a new
        // clone is created.
        let checkout = match GitRepository::open(destination)
            .ok()
            .map(|repo| GitCheckout::new(rev, repo))
            .filter(GitCheckout::is_fresh)
        {
            Some(co) => {
                if let Some(promisor_remote) = &self.promisor_remote {
                    // Refresh the checkout's promisor remote in case the URL changed,
                    // for example if credentials were updated.
                    co.repo
                        .configure_promisor_remote(CHECKOUT_REMOTE, promisor_remote)?;
                }

                if self.lfs_ready == Some(true) {
                    if co.repo.lfs_fsck_objects(rev.as_str()) {
                        co.with_lfs_ready(Some(true))
                    } else {
                        let lfs_ready = co.reset(
                            self.lfs_ready,
                            self.promisor_remote.as_ref(),
                            disable_ssl,
                            offline,
                        )?;
                        co.with_lfs_ready(lfs_ready)
                    }
                } else {
                    co.with_lfs_ready(self.lfs_ready)
                }
            }
            None => GitCheckout::clone_into(destination, self, rev, disable_ssl, offline)?,
        };
        Ok(checkout)
    }

    /// Get a short OID for a `revision`, usually 7 chars or more if ambiguous.
    pub(crate) fn to_short_id(&self, revision: GitOid) -> Result<String> {
        let output = GIT
            .as_ref()
            .cloned()?
            .arg("rev-parse")
            .arg("--short")
            .arg(revision.as_str())
            .cwd(&self.repo.path)
            .exec_with_output()?;

        let mut result = String::from_utf8(output.stdout)?;
        result.truncate(result.trim_end().len());
        Ok(result)
    }

    /// Checks if `oid` resolves to a commit in this database.
    pub(crate) fn contains(&self, oid: GitOid) -> bool {
        self.repo.rev_parse(&format!("{oid}^0")).is_ok()
    }

    /// Set the Git LFS validation state (if any).
    #[must_use]
    pub(crate) fn with_lfs_ready(mut self, lfs: Option<bool>) -> Self {
        self.lfs_ready = lfs;
        self
    }
}

impl GitCheckout {
    /// Creates an instance of [`GitCheckout`]. This doesn't imply the checkout
    /// is done. Use [`GitCheckout::is_fresh`] to check.
    ///
    /// * The `repo` will be the checked out Git repository.
    fn new(revision: GitOid, repo: GitRepository) -> Self {
        Self {
            revision,
            repo,
            lfs_ready: None,
        }
    }

    /// Clone a repo for a `revision` into a local path from a `database`.
    /// This is a filesystem-to-filesystem clone.
    fn clone_into(
        into: &Path,
        database: &GitDatabase,
        revision: GitOid,
        disable_ssl: bool,
        offline: bool,
    ) -> Result<Self> {
        let dirname = into.parent().unwrap();
        fs_err::create_dir_all(dirname)?;
        match fs_err::remove_dir_all(into) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }

        // Perform a local clone of the repository, which will attempt to use
        // hardlinks to set up the repository. This should speed up the clone operation
        // quite a bit if it works.
        let res = GIT
            .as_ref()
            .cloned()?
            .arg("clone")
            .arg("--local")
            // Make sure to pass the local file path and not a file://... url. If given a url,
            // Git treats the repository as a remote origin and gets confused because we don't
            // have a HEAD checked out.
            .arg(database.repo.path.simplified_display().to_string())
            .arg(into.simplified_display().to_string())
            .exec_with_output();

        if let Err(e) = res {
            debug!("Cloning git repo with --local failed, retrying without hardlinks: {e}");

            GIT.as_ref()
                .cloned()?
                .arg("clone")
                .arg("--no-hardlinks")
                .arg(database.repo.path.simplified_display().to_string())
                .arg(into.simplified_display().to_string())
                .exec_with_output()?;
        }

        let repo = GitRepository::open(into)?;
        if let Some(promisor_remote) = &database.promisor_remote {
            // Fetch missing objects from the original remote instead of the local
            // database clone, which may itself be a partial clone.
            repo.configure_promisor_remote(CHECKOUT_REMOTE, promisor_remote)?;
        }

        let checkout = Self::new(revision, repo);
        let lfs_ready = checkout.reset(
            database.lfs_ready,
            database.promisor_remote.as_ref(),
            disable_ssl,
            offline,
        )?;
        Ok(checkout.with_lfs_ready(lfs_ready))
    }

    /// Checks if the `HEAD` of this checkout points to the expected revision.
    fn is_fresh(&self) -> bool {
        match self.repo.rev_parse("HEAD") {
            Ok(id) if id == self.revision => {
                // See comments in reset() for why we check this
                self.repo.path.join(CHECKOUT_READY_LOCK).exists()
            }
            _ => false,
        }
    }

    /// Indicates Git LFS artifacts have been initialized (when requested).
    pub(crate) fn lfs_ready(&self) -> Option<bool> {
        self.lfs_ready
    }

    /// Set the Git LFS validation state (if any).
    #[must_use]
    pub(crate) fn with_lfs_ready(mut self, lfs: Option<bool>) -> Self {
        self.lfs_ready = lfs;
        self
    }

    /// This performs `git reset --hard` to the revision of this checkout, with
    /// additional interrupt protection by a dummy file [`CHECKOUT_READY_LOCK`].
    ///
    /// If we're interrupted while performing a `git reset` (e.g., we die
    /// because of a signal) uv needs to be sure to try to check out this
    /// repo again on the next go-round.
    ///
    /// To enable this we have a dummy file in our checkout, [`.ok`],
    /// which if present means that the repo has been successfully reset and is
    /// ready to go. Hence, if we start to do a reset, we make sure this file
    /// *doesn't* exist, and then once we're done we create the file.
    ///
    /// [`.ok`]: CHECKOUT_READY_LOCK
    fn reset(
        &self,
        with_lfs: Option<bool>,
        remote_url: Option<&DisplaySafeUrl>,
        disable_ssl: bool,
        offline: bool,
    ) -> Result<Option<bool>> {
        let ok_file = self.repo.path.join(CHECKOUT_READY_LOCK);
        let _ = paths::remove_file(&ok_file);

        // We want to skip smudge if lfs was disabled for the repository
        // as smudge filters can trigger on a reset even if lfs artifacts
        // were not originally "fetched".
        let lfs_skip_smudge = if with_lfs == Some(true) { "0" } else { "1" };
        debug!("Reset {} to {}", self.repo.path.display(), self.revision);

        // Perform the hard reset.
        let mut reset = GIT.as_ref().cloned()?;
        if let Some(remote_url) = remote_url {
            apply_url_rewrite(&mut reset, remote_url);
        }
        reset
            .arg("reset")
            .arg("--hard")
            .arg(self.revision.as_str())
            .env(EnvVars::GIT_LFS_SKIP_SMUDGE, lfs_skip_smudge)
            .env(EnvVars::GIT_TERMINAL_PROMPT, "0")
            .cwd(&self.repo.path);
        if disable_ssl {
            debug!("Disabling SSL verification for Git reset via `GIT_SSL_NO_VERIFY`");
            reset.env(EnvVars::GIT_SSL_NO_VERIFY, "true");
        }
        if offline {
            debug!("Disabling remote protocols for Git reset via `GIT_ALLOW_PROTOCOL=file`");
            reset.env(EnvVars::GIT_ALLOW_PROTOCOL, "file");
        }
        reset.exec_with_output().map_err(|err| {
            let msg = err.to_string();
            if msg.contains("transport '") && msg.contains("' not allowed") && offline {
                return GitError::TransportNotAllowed.into();
            }
            err
        })?;

        // Update submodules (`git submodule update --recursive`).
        let mut submodule_update = GIT.as_ref().cloned()?;
        if let Some(remote_url) = remote_url {
            apply_url_rewrite(&mut submodule_update, remote_url);
        }
        submodule_update
            .arg("submodule")
            .arg("update")
            .arg("--recursive")
            .arg("--init")
            .env(EnvVars::GIT_LFS_SKIP_SMUDGE, lfs_skip_smudge)
            .env(EnvVars::GIT_TERMINAL_PROMPT, "0")
            .cwd(&self.repo.path);
        if disable_ssl {
            debug!("Disabling SSL verification for Git submodule update via `GIT_SSL_NO_VERIFY`");
            submodule_update.env(EnvVars::GIT_SSL_NO_VERIFY, "true");
        }
        if offline {
            debug!(
                "Disabling remote protocols for Git submodule update via `GIT_ALLOW_PROTOCOL=file`"
            );
            submodule_update.env(EnvVars::GIT_ALLOW_PROTOCOL, "file");
        }
        submodule_update
            .exec_with_output()
            .map_err(|err| {
                let msg = err.to_string();
                if msg.contains("transport '") && msg.contains("' not allowed") && offline {
                    return GitError::TransportNotAllowed.into();
                }
                err
            })
            .map(drop)?;

        // Fetch and validate Git LFS objects (if needed) after the reset.
        // See `fetch_lfs` why we do this.
        let lfs_validation = match with_lfs {
            None => None,
            Some(false) => Some(false),
            Some(true) => {
                let lfs_ready = if let Some(remote_url) = remote_url {
                    fetch_lfs(&self.repo, remote_url, &self.revision, disable_ssl, offline)
                        .with_context(|| {
                            format!("failed to fetch LFS objects at {}", self.revision)
                        })?
                } else {
                    self.repo.lfs_fsck_objects(self.revision.as_str())
                };
                Some(lfs_ready)
            }
        };

        // The .ok file should be written when the reset is successful.
        // When Git LFS is enabled, the objects must also be fetched and
        // validated successfully as part of the corresponding checkout.
        if with_lfs.is_none() || lfs_validation == Some(true) {
            paths::create(ok_file)?;
        }

        Ok(lfs_validation)
    }
}

/// Returns the URL without embedded credentials for persistence in Git config.
fn without_credentials(url: &DisplaySafeUrl) -> DisplaySafeUrl {
    DisplaySafeUrl::from_url(url.without_credentials().into_owned())
}

/// Adds a one-shot Git URL rewrite so commands that perform lazy fetches can
/// authenticate without storing credentials in the repository config.
fn apply_url_rewrite(cmd: &mut ProcessBuilder, url: &DisplaySafeUrl) {
    let url_without_credentials = without_credentials(url);
    if url_without_credentials != *url {
        let config_index = env::var("GIT_CONFIG_COUNT")
            .ok()
            .and_then(|count| count.parse::<usize>().ok())
            .unwrap_or(0);
        let key_var = format!("GIT_CONFIG_KEY_{config_index}");
        let value_var = format!("GIT_CONFIG_VALUE_{config_index}");
        cmd.env("GIT_CONFIG_COUNT", (config_index + 1).to_string())
            .env(&key_var, format!("url.{}.insteadOf", url.as_str()))
            .env(&value_var, url_without_credentials.as_str());
    }
}

/// Attempts to fetch the given git `reference` for a Git repository.
///
/// This is the main entry for git clone/fetch. It does the following:
///
/// * Turns [`GitReference`] into refspecs accordingly.
/// * Dispatches `git fetch` using the git CLI.
///
/// The `remote_url` argument is the git remote URL where we want to fetch from.
fn fetch(
    repo: &mut GitRepository,
    remote_url: &DisplaySafeUrl,
    reference: ReferenceOrOid<'_>,
    disable_ssl: bool,
    offline: bool,
) -> Result<()> {
    let oid_to_fetch = if let ReferenceOrOid::Oid(rev) = reference {
        let local_object = reference.resolve(repo).ok();
        if let Some(local_object) = local_object {
            if rev == local_object {
                return Ok(());
            }
        }

        // If we know the reference is a full commit hash, we can just return it without
        // querying GitHub.
        Some(rev)
    } else {
        None
    };

    // Translate the reference desired here into an actual list of refspecs
    // which need to get fetched. Additionally record if we're fetching tags.
    let mut refspecs = Vec::new();
    let mut tags = false;
    let mut refspec_strategy = RefspecStrategy::All;
    // The `+` symbol on the refspec means to allow a forced (fast-forward)
    // update which is needed if there is ever a force push that requires a
    // fast-forward.
    match reference {
        // For branches and tags we can fetch simply one reference and copy it
        // locally, no need to fetch other branches/tags.
        ReferenceOrOid::Reference(GitReference::Branch(branch)) => {
            refspecs.push(format!("+refs/heads/{branch}:refs/remotes/origin/{branch}"));
        }

        ReferenceOrOid::Reference(GitReference::Tag(tag)) => {
            refspecs.push(format!("+refs/tags/{tag}:refs/remotes/origin/tags/{tag}"));
        }

        ReferenceOrOid::Reference(GitReference::BranchOrTag(branch_or_tag)) => {
            refspecs.push(format!(
                "+refs/heads/{branch_or_tag}:refs/remotes/origin/{branch_or_tag}"
            ));
            refspecs.push(format!(
                "+refs/tags/{branch_or_tag}:refs/remotes/origin/tags/{branch_or_tag}"
            ));
            refspec_strategy = RefspecStrategy::First;
        }

        // For ambiguous references, we can fetch the exact commit (if known); otherwise,
        // we fetch all branches and tags.
        ReferenceOrOid::Reference(GitReference::BranchOrTagOrCommit(branch_or_tag_or_commit)) => {
            // The `oid_to_fetch` is the exact commit we want to fetch. But it could be the exact
            // commit of a branch or tag. We should only fetch it directly if it's the exact commit
            // of a short commit hash.
            if let Some(oid_to_fetch) =
                oid_to_fetch.filter(|oid| is_short_hash_of(branch_or_tag_or_commit, *oid))
            {
                refspecs.push(format!("+{oid_to_fetch}:refs/commit/{oid_to_fetch}"));
            } else {
                // We don't know what the rev will point to. To handle this
                // situation we fetch all branches and tags, and then we pray
                // it's somewhere in there.
                refspecs.push(String::from("+refs/heads/*:refs/remotes/origin/*"));
                refspecs.push(String::from("+HEAD:refs/remotes/origin/HEAD"));
                tags = true;
            }
        }

        ReferenceOrOid::Reference(GitReference::DefaultBranch) => {
            refspecs.push(String::from("+HEAD:refs/remotes/origin/HEAD"));
        }

        ReferenceOrOid::Reference(GitReference::NamedRef(rev)) => {
            refspecs.push(format!("+{rev}:{rev}"));
        }

        ReferenceOrOid::Oid(rev) => {
            refspecs.push(format!("+{rev}:refs/commit/{rev}"));
        }
    }

    debug!("Performing a Git fetch for: {remote_url}");
    let result = match refspec_strategy {
        RefspecStrategy::All => fetch_refspecs(
            repo,
            remote_url,
            refspecs.as_slice(),
            tags,
            disable_ssl,
            offline,
        ),
        RefspecStrategy::First => {
            // Try each refspec
            let mut errors = refspecs
                .iter()
                .map_while(|refspec| {
                    let fetch_result = fetch_refspecs(
                        repo,
                        remote_url,
                        std::slice::from_ref(refspec),
                        tags,
                        disable_ssl,
                        offline,
                    );

                    // Stop after the first success and log failures
                    match fetch_result {
                        Err(ref err) => {
                            debug!("Failed to fetch refspec `{refspec}`: {err}");
                            Some(fetch_result)
                        }
                        Ok(()) => None,
                    }
                })
                .collect::<Vec<_>>();

            if errors.len() == refspecs.len() {
                if let Some(result) = errors.pop() {
                    // Use the last error for the message
                    result
                } else {
                    // Can only occur if there were no refspecs to fetch
                    Ok(())
                }
            } else {
                Ok(())
            }
        }
    };
    match reference {
        // With the default branch, adding context is confusing
        ReferenceOrOid::Reference(GitReference::DefaultBranch) => result,
        _ => result.with_context(|| {
            format!(
                "failed to fetch {} `{}`",
                reference.kind_str(),
                reference.as_rev()
            )
        }),
    }
}

/// Attempts to use `git` CLI installed on the system to fetch the given refspecs from a remote repository.
fn fetch_refspecs(
    repo: &mut GitRepository,
    url: &DisplaySafeUrl,
    refspecs: &[String],
    tags: bool,
    disable_ssl: bool,
    offline: bool,
) -> Result<()> {
    repo.configure_promisor_remote(CHECKOUT_REMOTE, url)?;

    let mut cmd = GIT.as_ref().cloned()?;
    // Disable interactive prompts in the terminal, as they'll be erased by the progress bar
    // animation and the process will "hang". Interactive prompts via the GUI like `SSH_ASKPASS`
    // are still usable.
    cmd.env(EnvVars::GIT_TERMINAL_PROMPT, "0");
    apply_url_rewrite(&mut cmd, url);

    cmd.arg("fetch");
    if tags {
        cmd.arg("--tags");
    }
    if disable_ssl {
        debug!("Disabling SSL verification for Git fetch via `GIT_SSL_NO_VERIFY`");
        cmd.env(EnvVars::GIT_SSL_NO_VERIFY, "true");
    }
    if offline {
        debug!("Disabling remote protocols for Git fetch via `GIT_ALLOW_PROTOCOL=file`");
        cmd.env(EnvVars::GIT_ALLOW_PROTOCOL, "file");
    }
    cmd.arg("--force") // handle force pushes
        .arg("--update-head-ok") // see discussion in #2078
        // Perform a treeless fetch, fetching trees and blobs on-demand.
        // We cannot perform a shallow clone because build tools such as
        // setuptools-scm may require access to git history, but we only
        // need the contents of the specific commit we are fetching.
        .arg(format!("--filter={PARTIAL_CLONE_FILTER}"))
        .arg(CHECKOUT_REMOTE)
        .args(refspecs)
        .cwd(&repo.path);

    // We capture the output to avoid streaming it to the user's console during clones.
    // The required `on...line` callbacks currently do nothing.
    // The output appears to be included in error messages by default.
    cmd.exec_with_output().map_err(|err| {
        let msg = err.to_string();
        if msg.contains("transport '") && msg.contains("' not allowed") && offline {
            return GitError::TransportNotAllowed.into();
        }
        err
    })?;

    Ok(())
}

/// A global cache of the `git lfs` command.
///
/// Returns an error if Git LFS isn't available.
/// Caching the command allows us to only check if LFS is installed once.
///
/// We also support a helper private environment variable to allow
/// controlling the LFS extension from being loaded for testing purposes.
/// Once installed, Git will always load `git-lfs` as a built-in alias
/// which takes priority over loading from `PATH` which prevents us
/// from shadowing the extension with other means.
pub static GIT_LFS: LazyLock<Result<ProcessBuilder>> = LazyLock::new(|| {
    if std::env::var_os(EnvVars::UV_INTERNAL__TEST_LFS_DISABLED).is_some() {
        return Err(anyhow!("Git LFS extension has been forcefully disabled."));
    }

    let mut cmd = GIT.as_ref()?.clone();
    cmd.arg("lfs");

    // Run a simple command to verify LFS is installed
    cmd.clone().arg("version").exec_with_output()?;
    Ok(cmd)
});

/// Attempts to use `git-lfs` CLI to fetch required LFS objects for a given revision.
fn fetch_lfs(
    repo: &GitRepository,
    url: &DisplaySafeUrl,
    revision: &GitOid,
    disable_ssl: bool,
    offline: bool,
) -> Result<bool> {
    let mut cmd = if let Ok(lfs) = GIT_LFS.as_ref() {
        debug!("Fetching Git LFS objects");
        lfs.clone()
    } else {
        // Since this feature is opt-in, warn if not available
        warn!("Git LFS is not available, skipping LFS fetch");
        return Ok(false);
    };

    if disable_ssl {
        debug!("Disabling SSL verification for Git LFS");
        cmd.env(EnvVars::GIT_SSL_NO_VERIFY, "true");
    }
    if offline {
        debug!("Disabling remote protocols for Git LFS via `GIT_ALLOW_PROTOCOL=file`");
        cmd.env(EnvVars::GIT_ALLOW_PROTOCOL, "file");
    }

    cmd.arg("fetch")
        .arg(url.as_str())
        .arg(revision.as_str())
        .env(EnvVars::GIT_TERMINAL_PROMPT, "0")
        // We should not support requesting LFS artifacts with skip smudge being set.
        // While this may not be necessary, it's added to avoid any potential future issues.
        .env_remove(EnvVars::GIT_LFS_SKIP_SMUDGE)
        .cwd(&repo.path);

    cmd.exec_with_output().map_err(|err| {
        let msg = err.to_string();
        if msg.contains("transport '") && msg.contains("' not allowed") && offline {
            return GitError::TransportNotAllowed.into();
        }
        err
    })?;

    // We now validate the Git LFS objects explicitly (if supported). This is
    // needed to avoid issues with Git LFS not being installed or configured
    // on the system and giving the wrong impression to the user that Git LFS
    // objects were initialized correctly when installation finishes.
    // We may want to allow the user to skip validation in the future via
    // UV_GIT_LFS_NO_VALIDATION environment variable on rare cases where
    // validation costs outweigh the benefit.
    let validation_result = repo.lfs_fsck_objects(revision.as_str());

    Ok(validation_result)
}

/// Whether `rev` is a shorter hash of `oid`.
fn is_short_hash_of(rev: &str, oid: GitOid) -> bool {
    let long_hash = oid.to_string();
    match long_hash.get(..rev.len()) {
        Some(truncated_long_hash) => truncated_long_hash.eq_ignore_ascii_case(rev),
        None => false,
    }
}
