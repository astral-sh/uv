//! Git support is derived from Cargo's implementation.
//! Cargo is dual-licensed under either Apache 2.0 or MIT, at the user's choice.
//! Source: <https://github.com/rust-lang/cargo/blob/23eb492cf920ce051abfc56bbaf838514dc8365c/src/cargo/sources/git/utils.rs>
use std::path::{Path, PathBuf, absolute};
use std::str::{self};
use std::sync::LazyLock;

use anyhow::{Context, Result, anyhow};
use cargo_util::{ProcessBuilder, ProcessError};
use owo_colors::OwoColorize;
use tracing::{debug, instrument, warn};

use uv_fs::Simplified;
use uv_git_types::{GitOid, GitReference};
use uv_redacted::SafeUrl;
use uv_static::EnvVars;
use uv_warnings::warn_user_once;

/// Extension for the marker beside a completed checkout.
/// See [`GitCheckout::reset`] for why we need this.
const CHECKOUT_READY_EXTENSION: &str = "ok";
/// Written after the root LFS objects have been verified and the files materialized.
const LFS_READY: &[u8] = b"lfs-v1";
/// Written after materialization when Git LFS lacks `fsck --objects`.
const LFS_UNCHECKED: &[u8] = b"lfs-v1-unchecked";
/// Git pathspec for files with the `filter=lfs` attribute.
const LFS_PATHSPEC: &str = ":(attr:filter=lfs)";

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

/// A remote repository. It gets cloned into a local [`GitDatabase`].
#[derive(PartialEq, Clone, Debug)]
pub(crate) struct GitRemote {
    /// URL to a remote repository.
    url: SafeUrl,
}

/// A local clone of a remote repository's database. Multiple [`GitCheckout`]s
/// can be cloned from a single [`GitDatabase`].
pub(crate) struct GitDatabase {
    /// The remote repository where this database is fetched from.
    remote: GitRemote,
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

/// The outcome of validating the LFS objects required by a revision.
enum LfsValidation {
    Passed,
    Failed,
    Unsupported,
}

impl GitRepository {
    fn at(path: &Path) -> Self {
        Self {
            path: path.to_path_buf(),
        }
    }

    /// Opens an existing Git repository at `path`.
    fn open(path: &Path) -> Result<Self> {
        // Make sure there is a Git repository at the specified path.
        GIT.as_ref()
            .cloned()?
            .arg("rev-parse")
            .cwd(path)
            .exec_with_output()?;

        Ok(Self::at(path))
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

    /// Resolve a [`GitReference`] using the objects already in this repository.
    fn resolve(&self, reference: &GitReference) -> Result<GitOid> {
        let result = match reference {
            // `^0` peels tags and verifies that the object refers to a commit.
            GitReference::Tag(tag) => self.rev_parse(&format!("refs/remotes/origin/tags/{tag}^0")),
            GitReference::Branch(branch) => self.rev_parse(&format!("origin/{branch}^0")),
            GitReference::BranchOrTag(reference) => self
                .rev_parse(&format!("origin/{reference}^0"))
                .or_else(|_| self.rev_parse(&format!("refs/remotes/origin/tags/{reference}^0"))),
            GitReference::BranchOrTagOrCommit(reference) => self
                .rev_parse(&format!("origin/{reference}^0"))
                .or_else(|_| self.rev_parse(&format!("refs/remotes/origin/tags/{reference}^0")))
                .or_else(|_| self.rev_parse(&format!("{reference}^0"))),
            GitReference::DefaultBranch => self.rev_parse("refs/remotes/origin/HEAD"),
            GitReference::NamedRef(reference) => self.rev_parse(&format!("{reference}^0")),
            GitReference::Commit(commit) => self.rev_parse(&format!("{commit}^0")),
        };

        result.with_context(|| format!("failed to find {} `{reference}`", reference.kind_str()))
    }

    /// Parses the object ID of the given `refname`.
    fn rev_parse(&self, refname: &str) -> Result<GitOid> {
        let result = GIT
            .as_ref()
            .cloned()?
            .args(&["--git-dir=.git", "rev-parse", refname])
            .cwd(&self.path)
            .exec_with_output()?;

        let mut result = String::from_utf8(result.stdout)?;
        result.truncate(result.trim_end().len());
        Ok(result.parse()?)
    }

    /// Reinsert LFS index entries without stat data so Git rewrites pointers and partial files.
    fn reset_lfs_index(&self) -> Result<bool> {
        let mut git = GIT.as_ref().cloned()?;
        let entries = git
            .clone()
            .args(&["ls-files", "--stage", "-z", "--", LFS_PATHSPEC])
            .cwd(&self.path)
            .exec_with_output()?;
        if entries.stdout.is_empty() {
            return Ok(false);
        }
        git.args(&["update-index", "-z", "--index-info"])
            .stdin(entries.stdout)
            .cwd(&self.path)
            .exec_with_output()?;
        Ok(true)
    }

    /// Materialize LFS pointers in an existing worktree using only local objects.
    fn materialize_lfs(&self) -> Result<()> {
        if self.reset_lfs_index()? {
            self.lfs_checkout(GIT.as_ref().cloned()?)?
                .args(&["checkout", "HEAD", "--", LFS_PATHSPEC])
                .cwd(&self.path)
                .exec_with_output()?;
        }
        Ok(())
    }

    /// Configure Git to materialize LFS files through a required local filter.
    /// `git lfs checkout` can report a short write on stderr and still exit successfully.
    fn lfs_checkout(&self, cmd: ProcessBuilder) -> Result<ProcessBuilder> {
        let mut config = GIT.as_ref().cloned()?;
        config.arg("config").cwd(&self.path);
        let clean = config
            .clone()
            .args(&["--get", "--default", "", "filter.lfs.clean"])
            .exec_with_output()?;
        if clean.stdout.trim_ascii().is_empty() {
            config
                .args(&["--local", "filter.lfs.clean", "git-lfs clean -- %f"])
                .exec_with_output()?;
        }

        let mut cmd = configure_local_lfs(cmd, self, &self.file_url()?)?;
        cmd.args(&[
            "-c",
            "filter.lfs.process=git-lfs filter-process",
            "-c",
            "filter.lfs.required=true",
            "-c",
            "lfs.skipdownloaderrors=false",
        ])
        .env("GIT_NO_LAZY_FETCH", "1")
        .env(EnvVars::GIT_ALLOW_PROTOCOL, "file")
        .env(EnvVars::GIT_LFS_SKIP_SMUDGE, "0")
        .env("GIT_LFS_SKIP_DOWNLOAD_ERRORS", "0");
        Ok(cmd)
    }

    fn file_url(&self) -> Result<SafeUrl> {
        let path = absolute(&self.path)?;
        SafeUrl::from_file_path(&path)
            .map_err(|()| anyhow!("Invalid Git repository path: {}", path.user_display()))
    }

    /// Verifies LFS artifacts when supported, warning when the installed LFS cannot check them.
    fn lfs_fsck_objects(&self, refname: &str) -> bool {
        match self.check_lfs_objects(refname) {
            LfsValidation::Passed | LfsValidation::Unsupported => true,
            LfsValidation::Failed => false,
        }
    }

    /// `git lfs fsck --objects` ignores `lfs.fetchinclude`. After it reports missing
    /// objects, a fetch from this repository's own LFS store checks the selected paths.
    fn missing_lfs_objects_are_unselected(
        &self,
        refname: &str,
        error: &anyhow::Error,
    ) -> Result<bool> {
        let Some(report) = error
            .downcast_ref::<ProcessError>()
            .and_then(|err| err.stdout.as_deref())
        else {
            return Ok(false);
        };
        // The repair summary follows a complete scan. A checksum mismatch still fails validation.
        let mut lines = report.split(|&byte| byte == b'\n');
        if !lines
            .clone()
            .any(|line| line.starts_with(b"objects: repair:"))
            || lines.any(|line| line.starts_with(b"objects: corruptObject:"))
        {
            return Ok(false);
        }

        let mut git = GIT.as_ref().cloned()?;
        git.cwd(&self.path)
            .env("GIT_NO_LAZY_FETCH", "1")
            .env(EnvVars::GIT_ALLOW_PROTOCOL, "file");
        let env = git.clone().args(&["lfs", "env"]).exec_with_output()?;
        if !env.stdout.split(|&byte| byte == b'\n').any(|line| {
            line.strip_prefix(b"FetchInclude=")
                .is_some_and(|value| !value.trim_ascii().is_empty())
        }) {
            return Ok(false);
        }

        let url = self.file_url()?;
        configure_local_lfs(git, self, &url)?
            .args(&["lfs", "fetch", url.as_str(), refname])
            .exec_with_output()?;
        Ok(true)
    }

    /// Check LFS objects, distinguishing an unsupported `fsck --objects` from failure.
    #[instrument(skip_all, fields(path = %self.path.user_display(), refname = %refname))]
    fn check_lfs_objects(&self, refname: &str) -> LfsValidation {
        let mut cmd = if let Ok(lfs) = GIT_LFS.as_ref() {
            lfs.clone()
        } else {
            warn!("Git LFS is not available, skipping LFS fetch");
            return LfsValidation::Failed;
        };

        // Requires Git LFS 3.x (2021 release)
        let result = cmd
            .arg("fsck")
            .arg("--objects")
            .arg(refname)
            .cwd(&self.path)
            .exec_with_output();

        match result {
            Ok(_) => LfsValidation::Passed,
            Err(err) => {
                let lfs_error = err.to_string();
                if lfs_error.contains("unknown flag: --objects") {
                    warn_user_once!(
                        "Skipping Git LFS validation as Git LFS extension is outdated. \
                        Upgrade to `git-lfs>=3.0.2` or manually verify git-lfs objects were \
                        properly fetched after the current operation finishes."
                    );
                    LfsValidation::Unsupported
                } else if self
                    .missing_lfs_objects_are_unselected(refname, &err)
                    .unwrap_or(false)
                {
                    LfsValidation::Passed
                } else {
                    debug!("Git LFS validation failed: {err}");
                    LfsValidation::Failed
                }
            }
        }
    }
}

impl GitRemote {
    /// Creates an instance for a remote repository URL.
    pub(crate) fn new(url: SafeUrl) -> Self {
        Self { url }
    }

    /// Gets the remote repository URL.
    pub(crate) fn url(&self) -> &SafeUrl {
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
        self,
        into: &Path,
        db: Option<GitDatabase>,
        reference: &GitReference,
        locked_rev: Option<GitOid>,
        disable_ssl: bool,
        offline: bool,
        with_lfs: bool,
    ) -> Result<(GitDatabase, GitOid)> {
        let locked_reference = locked_rev.map(GitReference::Commit);
        let reference = locked_reference.as_ref().unwrap_or(reference);
        if let Some(mut db) = db {
            fetch(&mut db.repo, &self.url, reference, disable_ssl, offline)
                .with_context(|| format!("failed to fetch into: {}", into.user_display()))?;

            if let Ok(rev) = db.repo.resolve(reference) {
                if with_lfs {
                    let lfs_ready =
                        fetch_lfs(&db.repo, &rev, LfsSource::Remote(&self.url, disable_ssl))
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
        let rev = repo.resolve(reference)?;
        let lfs_ready = with_lfs
            .then(|| {
                fetch_lfs(&repo, &rev, LfsSource::Remote(&self.url, disable_ssl))
                    .with_context(|| format!("failed to fetch LFS objects at {rev}"))
            })
            .transpose()?;

        Ok((
            GitDatabase {
                remote: self,
                repo,
                lfs_ready,
            },
            rev,
        ))
    }

    /// Creates a [`GitDatabase`] of this remote at `db_path`.
    pub(crate) fn db_at(&self, db_path: &Path) -> Result<GitDatabase> {
        let repo = GitRepository::open(db_path)?;
        Ok(GitDatabase {
            remote: self.clone(),
            repo,
            lfs_ready: None,
        })
    }
}

impl GitDatabase {
    /// Checkouts to a revision at `destination` from this database.
    pub(crate) fn copy_to(&self, rev: GitOid, destination: &Path) -> Result<GitCheckout> {
        if self.lfs_ready == Some(true)
            && let Some(checkout) = GitCheckout::open_lfs_ready(rev, destination)
        {
            return Ok(checkout);
        }
        // Interrupted checkouts lack the ready marker and must be cloned again.
        let Some(checkout) = GitCheckout::open_completed(rev, destination) else {
            return GitCheckout::clone_into(destination, self, rev);
        };
        if self.lfs_ready != Some(true) {
            return Ok(checkout.with_lfs_ready(self.lfs_ready));
        }

        // Clear index stat data so the reset rewrites stat-clean LFS pointers.
        GitCheckout::invalidate_ready(destination)?;
        checkout.repo.reset_lfs_index()?;
        checkout.reset(self)
    }

    /// Copy LFS objects from the shared database into a checkout clone.
    fn copy_lfs_to(&self, checkout: &GitCheckout) -> Result<Option<bool>> {
        if self.lfs_ready != Some(true) {
            return Ok(self.lfs_ready);
        }
        let url = self.repo.file_url()?;
        fetch_lfs(
            &checkout.repo,
            &checkout.revision,
            LfsSource::Database(&url),
        )
        .map(Some)
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

    /// Checks if `oid` identifies a commit object in this database.
    pub(crate) fn contains(&self, oid: GitOid) -> bool {
        self.repo
            .rev_parse(&format!("{oid}^0"))
            .is_ok_and(|commit| commit == oid)
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
    /// Create a checkout for the target revision.
    fn new(revision: GitOid, repo: GitRepository) -> Self {
        Self {
            revision,
            repo,
            lfs_ready: None,
        }
    }

    /// Invalidate readiness before replacing or repairing a checkout.
    fn invalidate_ready(path: &Path) -> Result<()> {
        match fs_err::remove_file(path.with_extension(CHECKOUT_READY_EXTENSION)) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
        }
    }

    /// Clone a repo for a `revision` into a local path from a `database`.
    /// This is a filesystem-to-filesystem clone.
    fn clone_into(into: &Path, database: &GitDatabase, revision: GitOid) -> Result<Self> {
        Self::invalidate_ready(into)?;

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
        let mut clone = GIT.as_ref().cloned()?;
        clone.arg("clone");
        if database.lfs_ready == Some(true) {
            // Copy cached LFS objects before checkout can run a smudge filter.
            clone.arg("--no-checkout");
        }
        let res = clone
            .clone()
            .arg("--local")
            // Make sure to pass the local file path and not a file://... url. If given a url,
            // Git treats the repository as a remote origin and gets confused because we don't
            // have a HEAD checked out.
            .arg(database.repo.path.simplified_display().to_string())
            .arg(into.simplified_display().to_string())
            .exec_with_output();

        if let Err(e) = res {
            debug!("Cloning git repo with --local failed, retrying without hardlinks: {e}");

            clone
                .arg("--no-hardlinks")
                .arg(database.repo.path.simplified_display().to_string())
                .arg(into.simplified_display().to_string())
                .exec_with_output()?;
        }

        Self::new(revision, GitRepository::open(into)?).reset(database)
    }

    /// Open a completed checkout at the full locked revision.
    fn open_completed(revision: GitOid, path: &Path) -> Option<Self> {
        if !path.with_extension(CHECKOUT_READY_EXTENSION).is_file() {
            return None;
        }
        let repo = GitRepository::at(path);
        (repo.rev_parse("HEAD").ok()? == revision).then(|| Self::new(revision, repo))
    }

    /// Reuse a completed LFS checkout, upgrading older markers using its local objects.
    pub(crate) fn open_lfs_ready(revision: GitOid, path: &Path) -> Option<Self> {
        Self::open_completed(revision, path)?
            .check_lfs_marker()
            .inspect_err(|err| {
                debug!(
                    "Could not reuse Git LFS checkout at {}: {err}",
                    path.display()
                );
            })
            .ok()
            .filter(|checkout| checkout.lfs_ready == Some(true))
    }

    fn check_lfs_marker(self) -> Result<Self> {
        let marker = fs_err::read(self.repo.path.with_extension(CHECKOUT_READY_EXTENSION))?;
        if marker == LFS_READY {
            return Ok(self.with_lfs_ready(Some(true)));
        }
        match self.repo.check_lfs_objects(self.revision.as_str()) {
            LfsValidation::Failed => Ok(self.with_lfs_ready(Some(false))),
            LfsValidation::Unsupported if marker == LFS_UNCHECKED => {
                Ok(self.with_lfs_ready(Some(true)))
            }
            validation => {
                // An older marker can belong to a checkout that still contains LFS pointers.
                Self::invalidate_ready(&self.repo.path)?;
                self.repo.materialize_lfs()?;
                self.finish(Some(validation))
            }
        }
    }

    /// Record a completed checkout and whether Git LFS verified it.
    fn finish(self, validation: Option<LfsValidation>) -> Result<Self> {
        let marker = match validation {
            Some(LfsValidation::Failed) => return Ok(self.with_lfs_ready(Some(false))),
            Some(LfsValidation::Passed) => LFS_READY,
            Some(LfsValidation::Unsupported) => LFS_UNCHECKED,
            None => b"",
        };
        fs_err::write(
            self.repo.path.with_extension(CHECKOUT_READY_EXTENSION),
            marker,
        )?;
        Ok(self.with_lfs_ready(validation.map(|_| true)))
    }

    /// Indicates Git LFS artifacts have been initialized (when requested).
    pub(crate) fn lfs_ready(&self) -> Option<bool> {
        self.lfs_ready
    }

    /// Set the Git LFS validation state (if any).
    #[must_use]
    fn with_lfs_ready(mut self, lfs: Option<bool>) -> Self {
        self.lfs_ready = lfs;
        self
    }

    /// This performs `git reset --hard` to the revision of this checkout and updates submodules,
    /// with additional interrupt protection by a marker file.
    ///
    /// If we're interrupted while performing any of the processes in this method (e.g., we die
    /// because of a signal) uv needs to be sure to try to check out this
    /// repo again on the next go-round.
    ///
    /// The marker sits beside the checkout with an [`.ok` extension], so tracked files cannot
    /// collide with it. It is removed before cloning and created only after preparation succeeds.
    /// For example, there may be the `checkouts/<repository-key>/0123456789abcdef/` contents
    /// directory containing `checkouts/<repository-key>/0123456789abcdef/pyproject.toml`, and the
    /// `checkouts/<repository-key>/0123456789abcdef.ok` marker file besides it.
    ///
    /// [`.ok` extension]: CHECKOUT_READY_EXTENSION
    /// `git reset --hard [<commit>]` can break relative submodule URLs, so we update submodules
    /// using the original remote URL.
    fn reset(self, database: &GitDatabase) -> Result<Self> {
        Self::invalidate_ready(&self.repo.path)?;
        let with_lfs = database.copy_lfs_to(&self)?;
        let original_remote_url = database.remote.url();

        // We want to skip smudge if lfs was disabled for the repository
        // as smudge filters can trigger on a reset even if lfs artifacts
        // were not originally "fetched".
        let lfs_skip_smudge = if with_lfs == Some(true) { "0" } else { "1" };

        debug!("Reset `{}` to {}", self.repo.path.display(), self.revision);

        let mut reset = GIT.as_ref().cloned()?;
        if with_lfs == Some(true) {
            // Materialize an LFS-backed `.gitmodules` before Git initializes submodules.
            reset = self.repo.lfs_checkout(reset)?;
        } else {
            reset.env(EnvVars::GIT_LFS_SKIP_SMUDGE, "1");
        }
        reset
            .args(&[
                "reset",
                "--hard",
                "--no-recurse-submodules",
                self.revision.as_str(),
            ])
            .cwd(&self.repo.path)
            .exec_with_output()?;

        // Initialize direct submodules using the original remote URL so Git can resolve relative
        // submodule URLs, but don't write it to `remote.origin.url`. Git persists resolved submodule
        // URLs during initialization, so writing a credentialed parent remote can leak credentials
        // into checkout configuration.
        //
        // Do not use `--recursive` here: command-local `remote.origin.url` config is inherited by
        // Git commands run inside submodules, which would make nested relative URLs resolve against
        // the top-level remote instead of their immediate parent submodule.
        let mut submodule_update = GIT.as_ref().cloned()?;
        for config in submodule_update_config(original_remote_url) {
            submodule_update.arg("-c").arg(config);
        }

        submodule_update
            .arg("submodule")
            .arg("update")
            .arg("--init")
            .env(EnvVars::GIT_LFS_SKIP_SMUDGE, lfs_skip_smudge)
            .cwd(&self.repo.path)
            .exec_with_output()
            .map_err(|err| redact_git_error(err, original_remote_url))
            .map(drop)?;

        // Recursively update nested submodules without overriding `remote.origin.url`, so each
        // nested relative URL resolves against its immediate parent submodule. The transient
        // credential rewrite is still safe to inherit because it only affects transport.
        let mut submodule_update = GIT.as_ref().cloned()?;
        for config in submodule_auth_config(original_remote_url) {
            submodule_update.arg("-c").arg(config);
        }

        submodule_update
            .arg("submodule")
            .arg("update")
            .arg("--recursive")
            .arg("--init")
            .env(EnvVars::GIT_LFS_SKIP_SMUDGE, lfs_skip_smudge)
            .cwd(&self.repo.path)
            .exec_with_output()
            .map_err(|err| redact_git_error(err, original_remote_url))
            .map(drop)?;

        match with_lfs {
            None => self.finish(None),
            Some(false) => Ok(self.with_lfs_ready(Some(false))),
            Some(true) => {
                let validation = self.repo.check_lfs_objects(self.revision.as_str());
                self.finish(Some(validation))
            }
        }
    }
}

/// Return command-local Git configuration for initializing direct submodules in a checkout.
///
/// Relative submodule URLs are resolved from `remote.origin.url`, but writing the original remote
/// URL into checkout configuration can persist credentials in the parent repository or submodule
/// remotes. Instead, callers pass these values via `git -c`, using a credential-stripped origin URL
/// for resolution and a transient `url.*.insteadOf` rewrite when credentials are needed for
/// transport.
fn submodule_update_config(original_remote_url: &SafeUrl) -> Vec<String> {
    let remote_url = original_remote_url.without_credentials();
    let mut config = vec![format!("remote.origin.url={}", remote_url.as_str())];

    config.extend(submodule_auth_config(original_remote_url));
    config
}

/// Return command-local Git authentication configuration for updating submodules.
///
/// Unlike `remote.origin.url`, these rewrites are safe to inherit during recursive submodule
/// updates: they rewrite transport URLs for authentication, but do not change the base URL that Git
/// uses to resolve nested relative submodule URLs.
fn submodule_auth_config(original_remote_url: &SafeUrl) -> Vec<String> {
    let remote_url = original_remote_url.without_credentials();
    let mut config = Vec::new();

    if remote_url.as_str() != original_remote_url.as_str() {
        let safe_root = remote_url_root(remote_url.into_owned());
        let credentialed_root = remote_url_root(original_remote_url.clone());

        if safe_root.as_str() != credentialed_root.as_str() {
            config.push(format!(
                "url.{}.insteadOf={}",
                credentialed_root.as_str(),
                safe_root.as_str()
            ));
        }
    }

    config
}

/// Return the scheme, authority, and root path of a remote URL.
///
/// This is used as the rewrite prefix for `url.*.insteadOf`, so a credentialed parent URL can
/// authenticate sibling submodule URLs without making the credentials part of any persisted
/// submodule URL.
fn remote_url_root(mut url: SafeUrl) -> SafeUrl {
    url.set_path("/");
    url.set_query(None);
    url.set_fragment(None);
    url
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
    remote_url: &SafeUrl,
    reference: &GitReference,
    disable_ssl: bool,
    offline: bool,
) -> Result<()> {
    if let GitReference::Commit(commit) = reference
        && repo
            .resolve(reference)
            .is_ok_and(|local_commit| *commit == local_commit)
    {
        return Ok(());
    }

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
        GitReference::Branch(branch) => {
            refspecs.push(format!("+refs/heads/{branch}:refs/remotes/origin/{branch}"));
        }

        GitReference::Tag(tag) => {
            refspecs.push(format!("+refs/tags/{tag}:refs/remotes/origin/tags/{tag}"));
        }

        GitReference::BranchOrTag(branch_or_tag) => {
            refspecs.push(format!(
                "+refs/heads/{branch_or_tag}:refs/remotes/origin/{branch_or_tag}"
            ));
            refspecs.push(format!(
                "+refs/tags/{branch_or_tag}:refs/remotes/origin/tags/{branch_or_tag}"
            ));
            refspec_strategy = RefspecStrategy::First;
        }

        // Fetch all branches and tags so ambiguous references can resolve to either a named
        // reference or a short commit hash.
        GitReference::BranchOrTagOrCommit(_) => {
            refspecs.push(String::from("+refs/heads/*:refs/remotes/origin/*"));
            refspecs.push(String::from("+HEAD:refs/remotes/origin/HEAD"));
            tags = true;
        }

        GitReference::DefaultBranch => {
            refspecs.push(String::from("+HEAD:refs/remotes/origin/HEAD"));
        }

        GitReference::NamedRef(rev) => {
            refspecs.push(format!("+{rev}:{rev}"));
        }

        GitReference::Commit(rev) => {
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
        GitReference::DefaultBranch => result,
        GitReference::Commit(_)
        | GitReference::Branch(_)
        | GitReference::Tag(_)
        | GitReference::BranchOrTag(_)
        | GitReference::BranchOrTagOrCommit(_)
        | GitReference::NamedRef(_) => result.with_context(|| {
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
    url: &SafeUrl,
    refspecs: &[String],
    tags: bool,
    disable_ssl: bool,
    offline: bool,
) -> Result<()> {
    let mut cmd = GIT.as_ref().cloned()?;
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
        .cwd(&repo.path);

    // We capture the output to avoid streaming it to the user's console during clones.
    // The required `on...line` callbacks currently do nothing.
    // The output appears to be included in error messages by default.
    cmd.exec_with_output().map_err(|err| {
        let msg = err.to_string();
        if msg.contains("transport '") && msg.contains("' not allowed") && offline {
            return GitError::TransportNotAllowed.into();
        }
        redact_git_error(err, url)
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

/// Append command-local configuration without replacing inherited entries.
fn append_git_config(cmd: &mut ProcessBuilder, key: &str, value: &str) {
    // Git before 2.31 accepts quoted `key=value` entries. Newer Git also accepts
    // separately quoted keys and values, which allows `=` in a key.
    let parameter = if key.contains('=') {
        format!("{}={}", quote_git_config(key), quote_git_config(value))
    } else {
        quote_git_config(&format!("{key}={value}"))
    };
    let mut parameters = cmd.get_env("GIT_CONFIG_PARAMETERS").unwrap_or_default();
    if !parameters.is_empty() {
        parameters.push(" ");
    }
    parameters.push(parameter);
    cmd.env("GIT_CONFIG_PARAMETERS", parameters);
}

/// Quote a string using Git's single-quote escaping rules.
fn quote_git_config(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''").replace('!', "'\\!'"))
}

#[derive(Clone, Copy)]
enum LfsSource<'a> {
    Remote(&'a SafeUrl, bool),
    Database(&'a SafeUrl),
}

/// Use Git LFS's built-in file adapter and override configured URL-specific adapters.
fn configure_local_lfs(
    mut cmd: ProcessBuilder,
    repo: &GitRepository,
    url: &SafeUrl,
) -> Result<ProcessBuilder> {
    // Use the local path as the LFS endpoint and force the built-in file adapter.
    cmd.arg("-c").arg(format!("lfs.url={url}"));
    // The local LFS store may lack objects from other recent revisions.
    cmd.arg("-c").arg("lfs.fetchrecentalways=false");
    append_git_config(
        &mut cmd,
        "lfs.standalonetransferagent",
        "lfs-standalone-file",
    );
    // A URL-specific selector can match an equivalent spelling of the local URL.
    // Override the configured selectors so a custom adapter cannot win for this copy.
    let adapters = GIT
        .as_ref()
        .cloned()?
        .args(&[
            "config",
            "--null",
            "--name-only",
            "--get-regexp",
            r"^lfs\..*\.standalonetransferagent$",
        ])
        .cwd(&repo.path)
        .exec_with_output();
    match adapters {
        Ok(output) => {
            for key in str::from_utf8(&output.stdout)?
                .split('\0')
                .filter(|key| !key.is_empty())
            {
                append_git_config(&mut cmd, key, "lfs-standalone-file");
            }
        }
        Err(error)
            if error
                .downcast_ref::<ProcessError>()
                .is_some_and(|error| error.code == Some(1)) => {}
        Err(error) => return Err(redact_git_error(error, url)),
    }
    Ok(cmd)
}

/// Attempts to use `git-lfs` CLI to fetch required LFS objects for a given revision.
fn fetch_lfs(repo: &GitRepository, revision: &GitOid, source: LfsSource<'_>) -> Result<bool> {
    let mut cmd = if GIT_LFS.as_ref().is_ok() {
        debug!("Fetching Git LFS objects");
        GIT.as_ref().cloned()?
    } else {
        // Since this feature is opt-in, warn if not available.
        warn!("Git LFS is not available, skipping LFS fetch");
        return Ok(false);
    };

    let (url, disable_ssl) = match source {
        LfsSource::Remote(url, disable_ssl) => (url, disable_ssl),
        LfsSource::Database(url) => {
            cmd = configure_local_lfs(cmd, repo, url)?;
            (url, false)
        }
    };
    if disable_ssl {
        debug!("Disabling SSL verification for Git LFS");
        cmd.env(EnvVars::GIT_SSL_NO_VERIFY, "true");
    }

    cmd.arg("lfs")
        .arg("fetch")
        .arg(url.as_str())
        .arg(revision.as_str())
        .env_remove(EnvVars::GIT_LFS_SKIP_SMUDGE)
        .cwd(&repo.path);

    cmd.exec_with_output()
        .map_err(|err| redact_git_error(err, url))?;

    match source {
        LfsSource::Remote(..) => Ok(repo.lfs_fsck_objects(revision.as_str())),
        // The checkout verifies the local copy after resetting to this revision.
        LfsSource::Database(_) => Ok(true),
    }
}

/// Redact a credentialed remote URL from a Git process error.
fn redact_git_error(mut error: anyhow::Error, url: &SafeUrl) -> anyhow::Error {
    let credentialed_root = remote_url_root(url.clone());
    let redact = |message: &str| credentialed_root.redact_in(&url.redact_in(message));

    if let Some(process_error) = error.downcast_mut::<ProcessError>() {
        process_error.desc = redact(&process_error.desc);
        return error;
    }

    anyhow!("{}", redact(&error.to_string()))
}

#[cfg(test)]
mod tests {
    use indoc::indoc;

    use super::*;

    #[test]
    fn submodule_update_config_strips_credentials_from_origin_override() {
        let url = SafeUrl::parse("https://user:password@example.com/org/repo.git").unwrap();

        assert_eq!(
            submodule_update_config(&url),
            vec![
                "remote.origin.url=https://example.com/org/repo.git".to_string(),
                "url.https://user:password@example.com/.insteadOf=https://example.com/".to_string(),
            ]
        );
    }

    #[test]
    fn submodule_update_config_preserves_git_ssh_user() {
        let url = SafeUrl::parse("ssh://git@example.com/org/repo.git").unwrap();

        assert_eq!(
            submodule_update_config(&url),
            vec!["remote.origin.url=ssh://git@example.com/org/repo.git".to_string()]
        );
    }

    #[test]
    fn git_process_error_redacts_credentials() -> Result<()> {
        let url = SafeUrl::parse("https://git:secret-token@example.com/org/repo.git")?;
        let stderr = format!("fatal: Authentication failed for '{}'", url.as_str());
        let error = ProcessError::new_raw(
            &format!(
                "process didn't exit successfully: `git fetch --force '{}' '+HEAD:refs/remotes/origin/HEAD'`",
                url.as_str()
            ),
            Some(128),
            "exit status: 128",
            Some(b"git output"),
            Some(stderr.as_bytes()),
        )
        .into();

        let error = redact_git_error(error, &url);
        let process_error = error
            .downcast_ref::<ProcessError>()
            .context("expected Git process error")?;

        assert_eq!(
            error.to_string(),
            indoc! {"
                process didn't exit successfully: `git fetch --force 'https://git:****@example.com/org/repo.git' '+HEAD:refs/remotes/origin/HEAD'` (exit status: 128)
                --- stdout
                git output
                --- stderr
                fatal: Authentication failed for 'https://git:****@example.com/org/repo.git'"
            }
        );
        assert_eq!(process_error.code, Some(128));
        assert_eq!(
            process_error.stdout.as_deref(),
            Some(b"git output".as_slice())
        );
        assert_eq!(process_error.stderr.as_deref(), Some(stderr.as_bytes()));

        Ok(())
    }

    #[test]
    fn git_submodule_process_error_redacts_credentials() -> Result<()> {
        let url = SafeUrl::parse("https://git:secret-token@example.com/org/repo.git")?;

        for args in ["--init", "--recursive --init"] {
            let error = anyhow!(
                "process didn't exit successfully: `git -c 'url.https://git:secret-token@example.com/.insteadOf=https://example.com/' submodule update {args}` (exit status: 128)"
            );
            let redacted = redact_git_error(error, &url).to_string();

            assert!(!redacted.contains("secret-token"));
            assert_eq!(
                redacted,
                format!(
                    "process didn't exit successfully: `git -c 'url.https://git:****@example.com/.insteadOf=https://example.com/' submodule update {args}` (exit status: 128)"
                )
            );
        }

        Ok(())
    }
}
