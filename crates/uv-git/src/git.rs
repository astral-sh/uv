//! Git support is derived from Cargo's implementation.
//! Cargo is dual-licensed under either Apache 2.0 or MIT, at the user's choice.
//! Source: <https://github.com/rust-lang/cargo/blob/23eb492cf920ce051abfc56bbaf838514dc8365c/src/cargo/sources/git/utils.rs>
use std::fmt::Display;
use std::path::{Path, PathBuf};
use std::str::{self};
use std::sync::LazyLock;

use anyhow::{Context, Result, anyhow};
use cargo_util::{ProcessBuilder, paths};
use owo_colors::OwoColorize;
use tracing::{debug, instrument, warn};
use url::Url;

use uv_fs::Simplified;
use uv_git_types::{GitOid, GitReference};
use uv_redacted::DisplaySafeUrl;
use uv_static::EnvVars;
use uv_warnings::warn_user_once;

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
        ProcessBuilder::new(GIT.as_ref()?)
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
        ProcessBuilder::new(GIT.as_ref()?)
            .arg("init")
            .cwd(path)
            .exec_with_output()?;

        Ok(Self {
            path: path.to_path_buf(),
        })
    }

    /// Parses the object ID of the given `refname`.
    fn rev_parse(&self, refname: &str) -> Result<GitOid> {
        let result = ProcessBuilder::new(GIT.as_ref()?)
            .arg("rev-parse")
            .arg(refname)
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
                if with_lfs {
                    let lfs_ready = fetch_lfs(&mut db.repo, &self.url, &rev, disable_ssl)
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
        let mut repo = GitRepository::init(into)?;
        fetch(&mut repo, &self.url, reference, disable_ssl, offline)
            .with_context(|| format!("failed to clone into: {}", into.user_display()))?;
        let rev = match locked_rev {
            Some(rev) => rev,
            None => reference.resolve(&repo)?,
        };
        let lfs_ready = with_lfs
            .then(|| {
                fetch_lfs(&mut repo, &self.url, &rev, disable_ssl)
                    .with_context(|| format!("failed to fetch LFS objects at {rev}"))
            })
            .transpose()?;

        Ok((GitDatabase { repo, lfs_ready }, rev))
    }

    /// Creates a [`GitDatabase`] of this remote at `db_path`.
    #[expect(clippy::unused_self)]
    pub(crate) fn db_at(&self, db_path: &Path) -> Result<GitDatabase> {
        let repo = GitRepository::open(db_path)?;
        Ok(GitDatabase {
            repo,
            lfs_ready: None,
        })
    }
}

impl GitDatabase {
    /// Checkouts to a revision at `destination` from this database.
    pub(crate) fn copy_to(&self, rev: GitOid, destination: &Path) -> Result<GitCheckout> {
        // If the existing checkout exists, and it is fresh, use it.
        // A non-fresh checkout can happen if the checkout operation was
        // interrupted. In that case, the checkout gets deleted and a new
        // clone is created.
        let checkout = match GitRepository::open(destination)
            .ok()
            .map(|repo| GitCheckout::new(rev, repo))
            .filter(GitCheckout::is_fresh)
        {
            Some(co) => co.with_lfs_ready(self.lfs_ready),
            None => GitCheckout::clone_into(destination, self, rev)?,
        };
        Ok(checkout)
    }

    /// Get a short OID for a `revision`, usually 7 chars or more if ambiguous.
    pub(crate) fn to_short_id(&self, revision: GitOid) -> Result<String> {
        let output = ProcessBuilder::new(GIT.as_ref()?)
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

    /// Checks if `oid` contains necessary LFS artifacts in this database.
    pub(crate) fn contains_lfs_artifacts(&self, oid: GitOid) -> bool {
        self.repo.lfs_fsck_objects(&format!("{oid}^0"))
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
    fn clone_into(into: &Path, database: &GitDatabase, revision: GitOid) -> Result<Self> {
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
        let res = ProcessBuilder::new(GIT.as_ref()?)
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

            ProcessBuilder::new(GIT.as_ref()?)
                .arg("clone")
                .arg("--no-hardlinks")
                .arg(database.repo.path.simplified_display().to_string())
                .arg(into.simplified_display().to_string())
                .exec_with_output()?;
        }

        let repo = GitRepository::open(into)?;
        let checkout = Self::new(revision, repo);
        let lfs_ready = checkout.reset(database.lfs_ready)?;
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
    fn reset(&self, with_lfs: Option<bool>) -> Result<Option<bool>> {
        let ok_file = self.repo.path.join(CHECKOUT_READY_LOCK);
        let _ = paths::remove_file(&ok_file);

        // We want to skip smudge if lfs was disabled for the repository
        // as smudge filters can trigger on a reset even if lfs artifacts
        // were not originally "fetched".
        let lfs_skip_smudge = if with_lfs == Some(true) { "0" } else { "1" };
        debug!("Reset {} to {}", self.repo.path.display(), self.revision);

        // Perform the hard reset.
        ProcessBuilder::new(GIT.as_ref()?)
            .arg("reset")
            .arg("--hard")
            .arg(self.revision.as_str())
            .env(EnvVars::GIT_LFS_SKIP_SMUDGE, lfs_skip_smudge)
            .cwd(&self.repo.path)
            .exec_with_output()?;

        // Update submodules (`git submodule update --recursive`).
        ProcessBuilder::new(GIT.as_ref()?)
            .arg("submodule")
            .arg("update")
            .arg("--recursive")
            .arg("--init")
            .env(EnvVars::GIT_LFS_SKIP_SMUDGE, lfs_skip_smudge)
            .cwd(&self.repo.path)
            .exec_with_output()
            .map(drop)?;

        // Validate Git LFS objects (if needed) after the reset.
        // See `fetch_lfs` why we do this.
        let lfs_validation = match with_lfs {
            None => None,
            Some(false) => Some(false),
            Some(true) => Some(self.repo.lfs_fsck_objects(self.revision.as_str())),
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
fn fetch(
    repo: &mut GitRepository,
    remote_url: &Url,
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
        RefspecStrategy::All => fetch_with_cli(
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
                    let fetch_result = fetch_with_cli(
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

/// Attempts to use `git` CLI installed on the system to fetch a repository.
fn fetch_with_cli(
    repo: &mut GitRepository,
    url: &Url,
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
    cmd.exec_with_output().map_err(|err| {
        let msg = err.to_string();

        // Check for offline mode transport error
        if msg.contains("transport '") && msg.contains("' not allowed") && offline {
            return GitError::TransportNotAllowed.into();
        }

        // Check for authentication/permission errors and provide helpful messages
        if is_authentication_error(&msg) {
            return anyhow::anyhow!(
                "Git authentication failed for `{url}`. \
                Ensure you have the correct credentials configured.\n\
                \n\
                For HTTPS repositories:\n  \
                - Check your Git credential helper: `git config --global credential.helper`\n  \
                - Or use a personal access token in the URL\n\
                \n\
                For SSH repositories:\n  \
                - Ensure your SSH key is added: `ssh-add -l`\n  \
                - Verify SSH access: `ssh -T git@<host>`\n\
                \n\
                Original error: {err}"
            );
        }

        // Check for repository not found errors
        if is_repository_not_found_error(&msg) {
            return anyhow::anyhow!(
                "Git repository not found: `{url}`.\n\
                \n\
                Please verify:\n  \
                - The repository URL is correct\n  \
                - You have access to the repository\n  \
                - The repository exists and is not private (or you have credentials configured)\n\
                \n\
                Original error: {err}"
            );
        }

        // Check for network/connectivity issues
        if is_network_error(&msg) {
            return anyhow::anyhow!(
                "Failed to connect to Git remote `{url}`.\n\
                \n\
                Please check:\n  \
                - Your network connection\n  \
                - The repository URL is correct\n  \
                - Any firewall or proxy settings\n\
                \n\
                Original error: {err}"
            );
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
fn fetch_lfs(
    repo: &mut GitRepository,
    url: &Url,
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

    cmd.exec_with_output()?;

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

/// Query the remote for the commit hash of a reference using `git ls-remote`.
///
/// This is a lightweight operation that only retrieves reference metadata without
/// downloading any objects. It works with HTTPS, SSH, and file protocols.
///
/// Returns `Ok(Some(oid))` if the reference exists and was resolved.
/// Returns `Ok(None)` if the reference type is ambiguous and cannot be queried directly.
/// Returns `Err` if the git command fails.
#[instrument(skip_all, fields(url = %url, reference = %reference))]
pub fn git_ls_remote(
    url: &Url,
    reference: &GitReference,
    disable_ssl: bool,
) -> Result<Option<GitOid>> {
    // Build the refspecs based on reference type
    let Some(refspecs) = reference_to_refspecs(reference) else {
        // For ambiguous refs (BranchOrTag, BranchOrTagOrCommit), we cannot
        // efficiently query with ls-remote as we'd need multiple queries.
        // Fall back to full fetch for these cases.
        debug!("Skipping ls-remote for ambiguous reference: {reference}");
        return Ok(None);
    };

    debug!("Running git ls-remote for {url} with refspecs {refspecs:?}");

    let mut cmd = ProcessBuilder::new(GIT.as_ref()?);
    cmd.arg("ls-remote");

    // Disable interactive prompts
    cmd.env(EnvVars::GIT_TERMINAL_PROMPT, "0");

    if disable_ssl {
        debug!("Disabling SSL verification for git ls-remote");
        cmd.env(EnvVars::GIT_SSL_NO_VERIFY, "true");
    }

    cmd.arg(url.as_str());
    for refspec in &refspecs {
        cmd.arg(refspec);
    }

    let output = match cmd.exec_with_output() {
        Ok(output) => output,
        Err(err) => {
            let err_msg = err.to_string();

            // Check for common authentication/permission error patterns
            if is_authentication_error(&err_msg) || is_repository_not_found_error(&err_msg) {
                return Err(err).with_context(|| {
                    format!(
                        "Git authentication failed for `{url}`. \
                        Ensure you have the correct credentials configured. \
                        For HTTPS: check your Git credential helper or use a personal access token. \
                        For SSH: ensure your SSH key is added to the ssh-agent and authorized on the server."
                    )
                });
            }

            // Check for network/connectivity issues
            if is_network_error(&err_msg) {
                return Err(err).with_context(|| {
                    format!(
                        "Failed to connect to Git remote `{url}`. \
                        Check your network connection and ensure the repository URL is correct."
                    )
                });
            }

            // Generic error
            return Err(err).with_context(|| {
                format!(
                    "failed to run `git ls-remote` for {} {}",
                    url,
                    reference.as_rev()
                )
            });
        }
    };

    // Parse the output: format is "<hash>\t<ref>\n" for each matching ref
    let stdout =
        String::from_utf8(output.stdout).context("git ls-remote output is not valid UTF-8")?;

    // The output may be empty if the ref doesn't exist
    if stdout.is_empty() {
        debug!("git ls-remote returned no results for {refspecs:?}");
        return Ok(None);
    }

    // Parse all lines and collect hash -> ref mappings
    // For annotated tags, we prefer the peeled ref (ending with ^{}) which points to the commit
    let mut result_oid: Option<GitOid> = None;
    let mut found_peeled = false;

    for line in stdout.lines() {
        let mut parts = line.split('\t');
        let Some(hash) = parts.next() else {
            continue;
        };
        let Some(ref_name) = parts.next() else {
            continue;
        };

        let hash = hash.trim();
        let ref_name = ref_name.trim();

        // Parse the hash
        let Ok(oid) = hash.parse::<GitOid>() else {
            continue;
        };

        // Prefer peeled refs (refs/tags/TAG^{}) for annotated tags
        // These point to the actual commit, not the tag object
        if ref_name.ends_with("^{}") {
            debug!("git ls-remote found peeled ref {ref_name} -> {oid}");
            result_oid = Some(oid);
            found_peeled = true;
        } else if !found_peeled {
            // Use non-peeled ref only if we haven't found a peeled one
            debug!("git ls-remote found ref {ref_name} -> {oid}");
            result_oid = Some(oid);
        }
    }

    if let Some(oid) = &result_oid {
        debug!("git ls-remote resolved {refspecs:?} to {oid}");
    }

    Ok(result_oid)
}

/// Check if an error message indicates an authentication failure.
fn is_authentication_error(msg: &str) -> bool {
    msg.contains("Authentication failed")
        || msg.contains("could not read Username")
        || msg.contains("Permission denied")
        || msg.contains("fatal: could not read Password")
        || msg.contains("Host key verification failed")
        || msg.contains("access denied")
        || msg.contains("The requested URL returned error: 403")
        || msg.contains("The requested URL returned error: 401")
}

/// Check if an error message indicates a repository not found error.
fn is_repository_not_found_error(msg: &str) -> bool {
    msg.contains("repository not found")
        || msg.contains("Repository not found")
        || msg.contains("does not appear to be a git repository")
}

/// Check if an error message indicates a network/connectivity error.
fn is_network_error(msg: &str) -> bool {
    msg.contains("Could not resolve host")
        || msg.contains("Connection refused")
        || msg.contains("Connection timed out")
        || msg.contains("unable to access")
        || msg.contains("Failed to connect")
}

/// Convert a [`GitReference`] to refspec strings for use with `git ls-remote`.
///
/// Returns `None` for ambiguous reference types that cannot be efficiently queried.
/// For tags, returns both the tag ref and the peeled ref (for annotated tags).
fn reference_to_refspecs(reference: &GitReference) -> Option<Vec<String>> {
    match reference {
        GitReference::Branch(branch) => Some(vec![format!("refs/heads/{branch}")]),
        GitReference::Tag(tag) => {
            // Query both the tag and its peeled version (for annotated tags).
            // The peeled ref (refs/tags/TAG^{}) points to the actual commit,
            // while the non-peeled ref may point to a tag object.
            Some(vec![
                format!("refs/tags/{tag}"),
                format!("refs/tags/{}^{{}}", tag),
            ])
        }
        GitReference::DefaultBranch => Some(vec!["HEAD".to_string()]),
        GitReference::NamedRef(named_ref) => Some(vec![named_ref.clone()]),
        GitReference::BranchOrTag(_) | GitReference::BranchOrTagOrCommit(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_authentication_error() {
        // Should detect authentication errors
        assert!(is_authentication_error(
            "fatal: Authentication failed for 'https://github.com/org/repo'"
        ));
        assert!(is_authentication_error(
            "fatal: could not read Username for 'https://github.com': terminal prompts disabled"
        ));
        assert!(is_authentication_error("Permission denied (publickey)."));
        assert!(is_authentication_error(
            "fatal: could not read Password for 'https://github.com': terminal prompts disabled"
        ));
        assert!(is_authentication_error("Host key verification failed."));
        assert!(is_authentication_error("remote: access denied"));
        assert!(is_authentication_error(
            "The requested URL returned error: 403"
        ));
        assert!(is_authentication_error(
            "The requested URL returned error: 401"
        ));

        // Should not detect non-auth errors
        assert!(!is_authentication_error("fatal: repository not found"));
        assert!(!is_authentication_error(
            "Could not resolve host: github.com"
        ));
        assert!(!is_authentication_error("Connection refused"));
    }

    #[test]
    fn test_is_repository_not_found_error() {
        // Should detect repo not found errors
        assert!(is_repository_not_found_error("ERROR: repository not found"));
        assert!(is_repository_not_found_error("Repository not found."));
        assert!(is_repository_not_found_error(
            "fatal: 'https://github.com/org/repo' does not appear to be a git repository"
        ));

        // Should not detect other errors
        assert!(!is_repository_not_found_error("Authentication failed"));
        assert!(!is_repository_not_found_error("Connection refused"));
    }

    #[test]
    fn test_is_network_error() {
        // Should detect network errors
        assert!(is_network_error(
            "fatal: Could not resolve host: github.com"
        ));
        assert!(is_network_error("fatal: Connection refused"));
        assert!(is_network_error("Connection timed out"));
        assert!(is_network_error(
            "fatal: unable to access 'https://github.com/org/repo/': Failed to connect"
        ));
        assert!(is_network_error("Failed to connect to github.com port 443"));

        // Should not detect other errors
        assert!(!is_network_error("Authentication failed"));
        assert!(!is_network_error("repository not found"));
    }

    #[test]
    fn test_reference_to_refspecs() {
        // Branch reference
        assert_eq!(
            reference_to_refspecs(&GitReference::Branch("main".to_string())),
            Some(vec!["refs/heads/main".to_string()])
        );
        assert_eq!(
            reference_to_refspecs(&GitReference::Branch("feature/test".to_string())),
            Some(vec!["refs/heads/feature/test".to_string()])
        );

        // Tag reference - should include both the tag and peeled version
        assert_eq!(
            reference_to_refspecs(&GitReference::Tag("v1.0.0".to_string())),
            Some(vec![
                "refs/tags/v1.0.0".to_string(),
                "refs/tags/v1.0.0^{}".to_string()
            ])
        );

        // Default branch (HEAD)
        assert_eq!(
            reference_to_refspecs(&GitReference::DefaultBranch),
            Some(vec!["HEAD".to_string()])
        );

        // Named ref
        assert_eq!(
            reference_to_refspecs(&GitReference::NamedRef("refs/pull/123/head".to_string())),
            Some(vec!["refs/pull/123/head".to_string()])
        );

        // Ambiguous references should return None
        assert_eq!(
            reference_to_refspecs(&GitReference::BranchOrTag("main".to_string())),
            None
        );
        assert_eq!(
            reference_to_refspecs(&GitReference::BranchOrTagOrCommit("abc123".to_string())),
            None
        );
    }

    #[test]
    fn test_is_short_hash_of() {
        let oid: GitOid = "4a23745badf5bf5ef7928f1e346e9986bd696d82".parse().unwrap();

        // Valid short hashes
        assert!(is_short_hash_of("4a23745", oid));
        assert!(is_short_hash_of(
            "4a23745badf5bf5ef7928f1e346e9986bd696d82",
            oid
        ));
        assert!(is_short_hash_of("4A23745", oid)); // case insensitive

        // Invalid short hashes
        assert!(!is_short_hash_of("1234567", oid));
        assert!(!is_short_hash_of(
            "4a23745badf5bf5ef7928f1e346e9986bd696d82a",
            oid
        )); // too long

        // Edge case: empty string technically matches (substring of length 0)
        // This is the current behavior, though it's unlikely to be used in practice
        assert!(is_short_hash_of("", oid));
    }
}
