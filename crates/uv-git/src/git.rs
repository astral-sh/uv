//! Git support is derived from Cargo's implementation.
//! Cargo is dual-licensed under either Apache 2.0 or MIT, at the user's choice.
//! Source: <https://github.com/rust-lang/cargo/blob/23eb492cf920ce051abfc56bbaf838514dc8365c/src/cargo/sources/git/utils.rs>
use std::fmt::Display;
use std::path::{Path, PathBuf};
use std::process::Output;
use std::str::{self};
use std::sync::LazyLock;

use anyhow::{Context, Result, anyhow};
use cargo_util::{ProcessBuilder, ProcessError, paths};
use owo_colors::OwoColorize;
use tokio::process::Command as AsyncCommand;
use tracing::{debug, instrument, warn};

use uv_fs::Simplified;
use uv_git_types::{GitOid, GitReference};
use uv_redacted::DisplaySafeUrl;
use uv_static::EnvVars;
use uv_warnings::warn_user_once;

/// Run a [`ProcessBuilder`] asynchronously via [`tokio::process::Command`].
///
/// This is the async equivalent of [`ProcessBuilder::exec_with_output`]: it pipes
/// stdout/stderr, awaits the child, and returns the captured [`Output`] on
/// success or an error describing the failure.
async fn exec_async(cmd: &ProcessBuilder) -> Result<Output> {
    let std_cmd = cmd.build_command();
    let mut async_cmd = AsyncCommand::from(std_cmd);
    async_cmd
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .stdin(std::process::Stdio::null());

    let output = async_cmd
        .output()
        .await
        .with_context(|| ProcessError::could_not_execute(cmd))?;

    if output.status.success() {
        Ok(output)
    } else {
        Err(ProcessError::new(
            &format!("process didn't exit successfully: {cmd}"),
            Some(output.status),
            Some(&output),
        )
        .into())
    }
}

/// A file indicates that if present, `git reset` has been done and a repo
/// checkout is ready to go. See [`GitCheckout::reset`] for why we need this.
const CHECKOUT_READY_LOCK: &str = ".ok";

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

/// A global cache of the result of `which git`.
pub static GIT: LazyLock<Result<PathBuf, GitError>> = LazyLock::new(|| {
    which::which("git").map_err(|err| match err {
        which::Error::CannotFindBinaryPath => GitError::GitNotFound,
        err => GitError::Other(err),
    })
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
    async fn resolve(&self, repo: &GitRepository) -> Result<GitOid> {
        let refkind = self.kind_str();
        let result = match self {
            // Resolve the commit pointed to by the tag.
            //
            // `^0` recursively peels away from the revision to the underlying commit object.
            // This also verifies that the tag indeed refers to a commit.
            Self::Reference(GitReference::Tag(s)) => {
                repo.rev_parse(&format!("refs/remotes/origin/tags/{s}^0"))
                    .await
            }

            // Resolve the commit pointed to by the branch.
            Self::Reference(GitReference::Branch(s)) => {
                repo.rev_parse(&format!("origin/{s}^0")).await
            }

            // Attempt to resolve the branch, then the tag.
            Self::Reference(GitReference::BranchOrTag(s)) => {
                let result = repo.rev_parse(&format!("origin/{s}^0")).await;
                if result.is_ok() {
                    result
                } else {
                    repo.rev_parse(&format!("refs/remotes/origin/tags/{s}^0"))
                        .await
                }
            }

            // Attempt to resolve the branch, then the tag, then the commit.
            Self::Reference(GitReference::BranchOrTagOrCommit(s)) => {
                let result = repo.rev_parse(&format!("origin/{s}^0")).await;
                if result.is_ok() {
                    result
                } else {
                    let result = repo
                        .rev_parse(&format!("refs/remotes/origin/tags/{s}^0"))
                        .await;
                    if result.is_ok() {
                        result
                    } else {
                        repo.rev_parse(&format!("{s}^0")).await
                    }
                }
            }

            // We'll be using the HEAD commit.
            Self::Reference(GitReference::DefaultBranch) => {
                repo.rev_parse("refs/remotes/origin/HEAD").await
            }

            // Resolve a named reference.
            Self::Reference(GitReference::NamedRef(s)) => repo.rev_parse(&format!("{s}^0")).await,

            // Resolve a specific commit.
            Self::Oid(s) => repo.rev_parse(&format!("{s}^0")).await,
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
    pub(crate) async fn open(path: &Path) -> Result<Self> {
        // Make sure there is a Git repository at the specified path.
        let mut cmd = ProcessBuilder::new(GIT.as_ref()?);
        cmd.arg("rev-parse").cwd(path);
        exec_async(&cmd).await?;

        Ok(Self {
            path: path.to_path_buf(),
        })
    }

    /// Initializes a Git repository at `path`.
    async fn init(path: &Path) -> Result<Self> {
        // TODO(ibraheem): see if this still necessary now that we no longer use libgit2
        // Skip anything related to templates, they just call all sorts of issues as
        // we really don't want to use them yet they insist on being used. See #6240
        // for an example issue that comes up.
        // opts.external_template(false);

        // Initialize the repository.
        let mut cmd = ProcessBuilder::new(GIT.as_ref()?);
        cmd.arg("init").cwd(path);
        exec_async(&cmd).await?;

        Ok(Self {
            path: path.to_path_buf(),
        })
    }

    /// Parses the object ID of the given `refname`.
    async fn rev_parse(&self, refname: &str) -> Result<GitOid> {
        let mut cmd = ProcessBuilder::new(GIT.as_ref()?);
        cmd.arg("rev-parse").arg(refname).cwd(&self.path);
        let result = exec_async(&cmd).await?;

        let mut result = String::from_utf8(result.stdout)?;
        result.truncate(result.trim_end().len());
        Ok(result.parse()?)
    }

    /// Verifies LFS artifacts have been initialized for a given `refname`.
    #[instrument(skip_all, fields(path = %self.path.user_display(), refname = %refname))]
    async fn lfs_fsck_objects(&self, refname: &str) -> bool {
        let mut cmd = if let Ok(lfs) = GIT_LFS.as_ref() {
            lfs.clone()
        } else {
            warn!("Git LFS is not available, skipping LFS fetch");
            return false;
        };

        // Requires Git LFS 3.x (2021 release)
        cmd.arg("fsck")
            .arg("--objects")
            .arg(refname)
            .cwd(&self.path);
        let result = exec_async(&cmd).await;

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
    pub(crate) async fn checkout(
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
                .await
                .with_context(|| format!("failed to fetch into: {}", into.user_display()))?;

            let resolved_commit_hash = match locked_rev {
                Some(rev) => db.contains(rev).await.then_some(rev),
                None => reference.resolve(&db.repo).await.ok(),
            };

            if let Some(rev) = resolved_commit_hash {
                if with_lfs {
                    let lfs_ready = fetch_lfs(&mut db.repo, &self.url, &rev, disable_ssl)
                        .await
                        .with_context(|| format!("failed to fetch LFS objects at {rev}"))?;
                    db = db.with_lfs_ready(Some(lfs_ready));
                }
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
        let mut repo = GitRepository::init(into).await?;
        fetch(&mut repo, &self.url, reference, disable_ssl, offline)
            .await
            .with_context(|| format!("failed to clone into: {}", into.user_display()))?;
        let rev = match locked_rev {
            Some(rev) => rev,
            None => reference.resolve(&repo).await?,
        };
        let lfs_ready = if with_lfs {
            Some(
                fetch_lfs(&mut repo, &self.url, &rev, disable_ssl)
                    .await
                    .with_context(|| format!("failed to fetch LFS objects at {rev}"))?,
            )
        } else {
            None
        };

        Ok((GitDatabase { repo, lfs_ready }, rev))
    }

    /// Creates a [`GitDatabase`] of this remote at `db_path`.
    pub(crate) async fn db_at(&self, db_path: &Path) -> Result<GitDatabase> {
        let repo = GitRepository::open(db_path).await?;
        Ok(GitDatabase {
            repo,
            lfs_ready: None,
        })
    }
}

impl GitDatabase {
    /// Checkouts to a revision at `destination` from this database.
    pub(crate) async fn copy_to(&self, rev: GitOid, destination: &Path) -> Result<GitCheckout> {
        // If the existing checkout exists, and it is fresh, use it.
        // A non-fresh checkout can happen if the checkout operation was
        // interrupted. In that case, the checkout gets deleted and a new
        // clone is created.
        let checkout = match GitRepository::open(destination).await {
            Ok(repo) => {
                let checkout = GitCheckout::new(rev, repo);
                if checkout.is_fresh().await {
                    checkout.with_lfs_ready(self.lfs_ready)
                } else {
                    GitCheckout::clone_into(destination, self, rev).await?
                }
            }
            Err(_) => GitCheckout::clone_into(destination, self, rev).await?,
        };
        Ok(checkout)
    }

    /// Get a short OID for a `revision`, usually 7 chars or more if ambiguous.
    pub(crate) async fn to_short_id(&self, revision: GitOid) -> Result<String> {
        let mut cmd = ProcessBuilder::new(GIT.as_ref()?);
        cmd.arg("rev-parse")
            .arg("--short")
            .arg(revision.as_str())
            .cwd(&self.repo.path);
        let output = exec_async(&cmd).await?;

        let mut result = String::from_utf8(output.stdout)?;
        result.truncate(result.trim_end().len());
        Ok(result)
    }

    /// Checks if `oid` resolves to a commit in this database.
    pub(crate) async fn contains(&self, oid: GitOid) -> bool {
        self.repo.rev_parse(&format!("{oid}^0")).await.is_ok()
    }

    /// Checks if `oid` contains necessary LFS artifacts in this database.
    pub(crate) async fn contains_lfs_artifacts(&self, oid: GitOid) -> bool {
        self.repo.lfs_fsck_objects(&format!("{oid}^0")).await
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
    async fn clone_into(into: &Path, database: &GitDatabase, revision: GitOid) -> Result<Self> {
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
        let mut cmd = ProcessBuilder::new(GIT.as_ref()?);
        cmd.arg("clone")
            .arg("--local")
            // Make sure to pass the local file path and not a file://... url. If given a url,
            // Git treats the repository as a remote origin and gets confused because we don't
            // have a HEAD checked out.
            .arg(database.repo.path.simplified_display().to_string())
            .arg(into.simplified_display().to_string());
        let res = exec_async(&cmd).await;

        if let Err(e) = res {
            debug!("Cloning git repo with --local failed, retrying without hardlinks: {e}");

            let mut cmd = ProcessBuilder::new(GIT.as_ref()?);
            cmd.arg("clone")
                .arg("--no-hardlinks")
                .arg(database.repo.path.simplified_display().to_string())
                .arg(into.simplified_display().to_string());
            exec_async(&cmd).await?;
        }

        let repo = GitRepository::open(into).await?;
        let checkout = Self::new(revision, repo);
        let lfs_ready = checkout.reset(database.lfs_ready).await?;
        Ok(checkout.with_lfs_ready(lfs_ready))
    }

    /// Checks if the `HEAD` of this checkout points to the expected revision.
    async fn is_fresh(&self) -> bool {
        match self.repo.rev_parse("HEAD").await {
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
    async fn reset(&self, with_lfs: Option<bool>) -> Result<Option<bool>> {
        let ok_file = self.repo.path.join(CHECKOUT_READY_LOCK);
        let _ = paths::remove_file(&ok_file);

        // We want to skip smudge if lfs was disabled for the repository
        // as smudge filters can trigger on a reset even if lfs artifacts
        // were not originally "fetched".
        let lfs_skip_smudge = if with_lfs == Some(true) { "0" } else { "1" };
        debug!("Reset {} to {}", self.repo.path.display(), self.revision);

        // Perform the hard reset.
        let mut cmd = ProcessBuilder::new(GIT.as_ref()?);
        cmd.arg("reset")
            .arg("--hard")
            .arg(self.revision.as_str())
            .env(EnvVars::GIT_LFS_SKIP_SMUDGE, lfs_skip_smudge)
            .cwd(&self.repo.path);
        exec_async(&cmd).await?;

        // Update submodules (`git submodule update --recursive`).
        let mut cmd = ProcessBuilder::new(GIT.as_ref()?);
        cmd.arg("submodule")
            .arg("update")
            .arg("--recursive")
            .arg("--init")
            .env(EnvVars::GIT_LFS_SKIP_SMUDGE, lfs_skip_smudge)
            .cwd(&self.repo.path);
        exec_async(&cmd).await?;

        // Validate Git LFS objects (if needed) after the reset.
        // See `fetch_lfs` why we do this.
        let lfs_validation = match with_lfs {
            None => None,
            Some(false) => Some(false),
            Some(true) => Some(self.repo.lfs_fsck_objects(self.revision.as_str()).await),
        };

        // The .ok file should be written when the reset is successful.
        // When Git LFS is enabled, the objects must also be fetched and
        // validated successfully as part of the corresponding db.
        if with_lfs.is_none() || lfs_validation == Some(true) {
            paths::create(ok_file)?;
        }

        Ok(lfs_validation)
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
async fn fetch(
    repo: &mut GitRepository,
    remote_url: &DisplaySafeUrl,
    reference: ReferenceOrOid<'_>,
    disable_ssl: bool,
    offline: bool,
) -> Result<()> {
    let oid_to_fetch = if let ReferenceOrOid::Oid(rev) = reference {
        let local_object = reference.resolve(repo).await.ok();
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
        RefspecStrategy::All => {
            fetch_with_cli(
                repo,
                remote_url,
                refspecs.as_slice(),
                tags,
                disable_ssl,
                offline,
            )
            .await
        }
        RefspecStrategy::First => {
            // Try each refspec, stopping after the first success.
            let mut errors = Vec::new();
            for refspec in &refspecs {
                let fetch_result = fetch_with_cli(
                    repo,
                    remote_url,
                    std::slice::from_ref(refspec),
                    tags,
                    disable_ssl,
                    offline,
                )
                .await;

                match fetch_result {
                    Ok(()) => break,
                    Err(ref err) => {
                        debug!("Failed to fetch refspec `{refspec}`: {err}");
                        errors.push(fetch_result);
                    }
                }
            }

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

/// Attempts to use `git` CLI installed on the system to fetch a repository.
async fn fetch_with_cli(
    repo: &mut GitRepository,
    url: &DisplaySafeUrl,
    refspecs: &[String],
    tags: bool,
    disable_ssl: bool,
    offline: bool,
) -> Result<()> {
    let mut cmd = ProcessBuilder::new(GIT.as_ref()?);
    // Disable interactive prompts in the terminal, as they'll be erased by the progress bar
    // animation and the process will "hang". Interactive prompts via the GUI like `SSH_ASKPASS`
    // are still usable.
    cmd.env(EnvVars::GIT_TERMINAL_PROMPT, "0");

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
        .arg(url.as_str())
        .args(refspecs)
        // If cargo is run by git (for example, the `exec` command in `git
        // rebase`), the GIT_DIR is set by git and will point to the wrong
        // location (this takes precedence over the cwd). Make sure this is
        // unset so git will look at cwd for the repo.
        .env_remove(EnvVars::GIT_DIR)
        // The reset of these may not be necessary, but I'm including them
        // just to be extra paranoid and avoid any issues.
        .env_remove(EnvVars::GIT_WORK_TREE)
        .env_remove(EnvVars::GIT_INDEX_FILE)
        .env_remove(EnvVars::GIT_OBJECT_DIRECTORY)
        .env_remove(EnvVars::GIT_ALTERNATE_OBJECT_DIRECTORIES)
        .cwd(&repo.path);

    // We capture the output to avoid streaming it to the user's console during clones.
    // The required `on...line` callbacks currently do nothing.
    // The output appears to be included in error messages by default.
    exec_async(&cmd).await.map_err(|err| {
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

    let mut cmd = ProcessBuilder::new(GIT.as_ref()?);
    cmd.arg("lfs");

    // Run a simple command to verify LFS is installed
    cmd.clone().arg("version").exec_with_output()?;
    Ok(cmd)
});

/// Attempts to use `git-lfs` CLI to fetch required LFS objects for a given revision.
async fn fetch_lfs(
    repo: &mut GitRepository,
    url: &DisplaySafeUrl,
    revision: &GitOid,
    disable_ssl: bool,
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

    cmd.arg("fetch")
        .arg(url.as_str())
        .arg(revision.as_str())
        // These variables are unset for the same reason as in `fetch_with_cli`.
        .env_remove(EnvVars::GIT_DIR)
        .env_remove(EnvVars::GIT_WORK_TREE)
        .env_remove(EnvVars::GIT_INDEX_FILE)
        .env_remove(EnvVars::GIT_OBJECT_DIRECTORY)
        .env_remove(EnvVars::GIT_ALTERNATE_OBJECT_DIRECTORIES)
        // We should not support requesting LFS artifacts with skip smudge being set.
        // While this may not be necessary, it's added to avoid any potential future issues.
        .env_remove(EnvVars::GIT_LFS_SKIP_SMUDGE)
        .cwd(&repo.path);

    exec_async(&cmd).await?;

    // We now validate the Git LFS objects explicitly (if supported). This is
    // needed to avoid issues with Git LFS not being installed or configured
    // on the system and giving the wrong impression to the user that Git LFS
    // objects were initialized correctly when installation finishes.
    // We may want to allow the user to skip validation in the future via
    // UV_GIT_LFS_NO_VALIDATION environment variable on rare cases where
    // validation costs outweigh the benefit.
    let validation_result = repo.lfs_fsck_objects(revision.as_str()).await;

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
