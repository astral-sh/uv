//! Git support is derived from Cargo's implementation.
//! Cargo is dual-licensed under either Apache 2.0 or MIT, at the user's choice.
//! Source: <https://github.com/rust-lang/cargo/blob/23eb492cf920ce051abfc56bbaf838514dc8365c/src/cargo/sources/git/utils.rs>
use std::path::{Path, PathBuf, absolute};
use std::str::{self};
use std::sync::LazyLock;

use anyhow::{Context, Result, anyhow, ensure};
use cargo_util::{ProcessBuilder, ProcessError, paths};
use owo_colors::OwoColorize;
use tracing::{debug, instrument, warn};

use uv_auth::TrustedHost;
use uv_fs::Simplified;
use uv_git_types::{GitOid, GitReference};
use uv_preview::PreviewFeature;
use uv_redacted::SafeUrl;
use uv_static::EnvVars;
use uv_warnings::warn_user_once;

use crate::GitHttpSettings;

/// Extension for the marker beside a completed checkout.
/// See [`GitCheckout::reset`] for why we need this.
const CHECKOUT_READY_EXTENSION: &str = "ok";

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
    /// Whether this instance was created by [`Self::init`].
    newly_initialized: bool,
}

/// The outcome of validating the LFS objects required by a revision.
enum LfsValidation {
    Passed,
    Failed,
    Unsupported,
}

impl GitRepository {
    /// Opens an existing Git repository at `path`.
    fn open(path: &Path) -> Result<Self> {
        // Make sure there is a Git repository at the specified path.
        GIT.as_ref()
            .cloned()?
            .arg("rev-parse")
            .cwd(path)
            .exec_with_output()?;

        Ok(Self {
            path: path.to_path_buf(),
            newly_initialized: false,
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
            newly_initialized: true,
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

    /// Returns whether a promisor remote is configured for this repository.
    fn has_promisor_remote(&self) -> Result<bool> {
        let result = GIT
            .as_ref()
            .cloned()?
            .arg("config")
            .arg("--bool")
            .arg("--get-regexp")
            .arg(r"^remote\..*\.promisor$")
            .cwd(&self.path)
            .exec_with_output();
        match result {
            Ok(output) => Ok(str::from_utf8(&output.stdout)?
                .lines()
                .any(|line| line.ends_with(" true"))),
            Err(error)
                if error
                    .downcast_ref::<ProcessError>()
                    .is_some_and(|error| error.code == Some(1)) =>
            {
                Ok(false)
            }
            Err(error) => Err(error),
        }
    }

    fn set_remote_url(&self, url: &SafeUrl) -> Result<()> {
        GIT.as_ref()
            .cloned()?
            .arg("config")
            .arg(format!("remote.{CHECKOUT_REMOTE}.url"))
            .arg(url.without_credentials().as_str())
            .cwd(&self.path)
            .exec_with_output()?;
        Ok(())
    }

    /// Parses the object ID of the given `refname`.
    fn rev_parse(&self, refname: &str) -> Result<GitOid> {
        let mut command = GIT.as_ref().cloned()?;
        command.arg("rev-parse").arg(refname);
        if uv_preview::is_enabled(PreviewFeature::GitPartialFetches) {
            // Revision probes must not fetch missing objects. Older Git ignores `GIT_NO_LAZY_FETCH`.
            command
                .env("GIT_NO_LAZY_FETCH", "1")
                .env(EnvVars::GIT_ALLOW_PROTOCOL, "");
        }
        let result = command.cwd(&self.path).exec_with_output()?;

        let mut result = String::from_utf8(result.stdout)?;
        result.truncate(result.trim_end().len());
        Ok(result.parse()?)
    }

    /// Materialize LFS files and configure a missing clean filter for later Git commands.
    fn materialize_lfs(&self) -> Result<()> {
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

        GIT_LFS
            .as_ref()
            .map_err(|_| GitError::GitLfsNotFound)?
            .clone()
            .arg("checkout")
            .env("GIT_NO_LAZY_FETCH", "1")
            .env(EnvVars::GIT_ALLOW_PROTOCOL, "file")
            .env_remove(EnvVars::GIT_LFS_SKIP_SMUDGE)
            .cwd(&self.path)
            .exec_with_output()?;
        Ok(())
    }

    /// Resolve the LFS endpoint using Git LFS's configuration and URL rewriting rules.
    fn lfs_endpoint(&self, remote: &str) -> Result<SafeUrl> {
        let mut cmd = GIT.as_ref().cloned()?;
        // Make the exact fetch argument appear in `git lfs env` without changing its URL.
        append_git_config(&mut cmd, &format!("remote.{remote}.uv-lfs-probe"), "true");
        let prefix = if remote == "origin" {
            append_git_config(&mut cmd, "remote.lfsdefault", "origin");
            "Endpoint=".to_owned()
        } else {
            format!("Endpoint ({remote})=")
        };
        let output = cmd
            .args(&["lfs", "env"])
            .env("GIT_NO_LAZY_FETCH", "1")
            .env(EnvVars::GIT_ALLOW_PROTOCOL, "file")
            .cwd(&self.path)
            .exec_with_output()
            // The remaining output can contain credentials and arbitrary environment values.
            .map_err(|_| anyhow!("failed to determine Git LFS endpoint"))?;
        let endpoint = str::from_utf8(&output.stdout)?
            .lines()
            .find_map(|line| line.strip_prefix(&prefix))
            .and_then(|line| line.rsplit_once(" (auth=").map(|(url, _)| url))
            .context("Git LFS did not report its endpoint")?;
        Ok(SafeUrl::parse(endpoint)?)
    }

    /// Return whether an LFS smudge or process filter is configured.
    fn has_lfs_smudge_filter(&self) -> Result<bool> {
        let filters = GIT
            .as_ref()
            .cloned()?
            .args(&[
                "config",
                "--null",
                "--get-regexp",
                r"^filter\.lfs\.(process|smudge)$",
            ])
            .cwd(&self.path)
            .exec_with_output();
        match filters {
            Ok(output) => {
                let mut process = false;
                let mut smudge = false;
                for entry in output.stdout.split(|&byte| byte == 0) {
                    let mut fields = entry.splitn(2, |&byte| byte == b'\n');
                    let key = fields.next().unwrap_or_default();
                    let enabled = fields
                        .next()
                        .is_some_and(|value| !value.trim_ascii().is_empty());
                    match key {
                        b"filter.lfs.process" => process = enabled,
                        b"filter.lfs.smudge" => smudge = enabled,
                        _ => {}
                    }
                }
                Ok(process || smudge)
            }
            Err(error)
                if error
                    .downcast_ref::<ProcessError>()
                    .is_some_and(|error| error.code == Some(1)) =>
            {
                Ok(false)
            }
            Err(error) => Err(error),
        }
    }

    /// Verifies LFS artifacts when supported, warning when the installed LFS cannot check them.
    fn lfs_fsck_objects(&self, refname: &str) -> bool {
        match self.check_lfs_objects(refname) {
            LfsValidation::Passed | LfsValidation::Unsupported => true,
            LfsValidation::Failed => false,
        }
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

        if uv_preview::is_enabled(PreviewFeature::GitPartialFetches) {
            cmd.env("GIT_NO_LAZY_FETCH", "1")
                .env(EnvVars::GIT_ALLOW_PROTOCOL, "file");
        }
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
        settings: &GitHttpSettings,
        with_lfs: bool,
    ) -> Result<(GitDatabase, GitOid)> {
        let locked_reference = locked_rev.map(GitReference::Commit);
        let reference = locked_reference.as_ref().unwrap_or(reference);
        if let Some(mut db) = db {
            fetch(&mut db.repo, &self.url, reference, settings)
                .with_context(|| format!("failed to fetch into: {}", into.user_display()))?;

            if let Ok(rev) = db.repo.resolve(reference) {
                if with_lfs {
                    let lfs_ready =
                        fetch_lfs(&db.repo, &rev, LfsSource::Remote(&self.url, settings))
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
        fetch(&mut repo, &self.url, reference, settings)
            .with_context(|| format!("failed to clone into: {}", into.user_display()))?;
        let rev = repo.resolve(reference)?;
        let lfs_ready = with_lfs
            .then(|| {
                fetch_lfs(&repo, &rev, LfsSource::Remote(&self.url, settings))
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
    pub(crate) fn copy_to(
        &self,
        rev: GitOid,
        destination: &Path,
        settings: &GitHttpSettings,
    ) -> Result<GitCheckout> {
        // Interrupted checkouts lack the ready marker and must be cloned again.
        let Some(checkout) = GitRepository::open(destination)
            .ok()
            .map(|repo| GitCheckout::new(rev, repo))
            .filter(GitCheckout::is_fresh)
        else {
            return GitCheckout::clone_into(destination, self, rev, settings);
        };
        if self.lfs_ready != Some(true) {
            return Ok(checkout.with_lfs_ready(self.lfs_ready));
        }

        GitCheckout::invalidate_ready(&checkout.repo.path)?;
        if !checkout.repo.lfs_fsck_objects(rev.as_str()) {
            return checkout.reset(self, settings);
        }
        let ready = checkout.materialize_lfs()?;
        if ready {
            paths::create(checkout.repo.path.with_extension(CHECKOUT_READY_EXTENSION))?;
        }
        Ok(checkout.with_lfs_ready(Some(ready)))
    }

    /// Copy LFS objects from the shared database into a checkout clone.
    fn copy_lfs_to(&self, checkout: &GitCheckout) -> Result<Option<bool>> {
        if self.lfs_ready != Some(true) {
            return Ok(self.lfs_ready);
        }
        let path = absolute(&self.repo.path)?;
        let url = SafeUrl::from_file_path(&path)
            .map_err(|()| anyhow!("Invalid Git database path: {}", path.user_display()))?;
        fetch_lfs(&checkout.repo, &checkout.revision, LfsSource::Local(&url)).map(Some)
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
    fn clone_into(
        into: &Path,
        database: &GitDatabase,
        revision: GitOid,
        settings: &GitHttpSettings,
    ) -> Result<Self> {
        Self::invalidate_ready(into)?;

        let dirname = into.parent().unwrap();
        fs_err::create_dir_all(dirname)?;
        match fs_err::remove_dir_all(into) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }

        let partial_fetches = settings.allows_partial_fetches(database.remote.url());
        // Perform a local clone of the repository, which will attempt to use
        // hardlinks to set up the repository. This should speed up the clone operation
        // quite a bit if it works.
        let mut clone = GIT.as_ref().cloned()?;
        clone.arg("clone");
        if partial_fetches {
            // Cache this revision's trees and blobs for local clones, including offline reuse.
            fetch_checkout_objects(&database.repo, database.remote.url(), &revision, settings)?;
            clone
                .arg("--config")
                .arg(format!("remote.{CHECKOUT_REMOTE}.promisor=true"))
                .arg("--config")
                .arg(format!(
                    "remote.{CHECKOUT_REMOTE}.partialclonefilter={PARTIAL_CLONE_FILTER}"
                ));
        }
        if partial_fetches || database.lfs_ready == Some(true) {
            // Configure the promisor remote and copy cached LFS objects before checking out files.
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

        Self::new(revision, GitRepository::open(into)?).reset(database, settings)
    }

    /// Checks if the `HEAD` of this checkout points to the expected revision.
    fn is_fresh(&self) -> bool {
        match self.repo.rev_parse("HEAD") {
            Ok(id) if id == self.revision => {
                // See comments in reset() for why we check this
                self.repo
                    .path
                    .with_extension(CHECKOUT_READY_EXTENSION)
                    .exists()
            }
            _ => false,
        }
    }

    /// Materialize available local LFS objects and check initialized submodules recursively when supported.
    fn materialize_lfs(&self) -> Result<bool> {
        self.repo.materialize_lfs()?;
        let mut ready = true;
        for repo in self.submodules()? {
            repo.materialize_lfs()?;
            // `git lfs checkout` skips missing objects without failing.
            ready &= repo.lfs_fsck_objects("HEAD");
        }
        Ok(ready)
    }

    /// Fetch submodule LFS objects from file endpoints during offline checkout.
    fn fetch_local_submodule_lfs(&self) -> Result<()> {
        for repo in self.submodules()? {
            match repo.check_lfs_objects("HEAD") {
                LfsValidation::Passed => continue,
                LfsValidation::Failed | LfsValidation::Unsupported => {}
            }
            if !repo.has_lfs_smudge_filter()? {
                continue;
            }
            let endpoint = repo.lfs_endpoint("origin")?;
            if endpoint.scheme() != "file" {
                return Err(GitError::TransportNotAllowed.into());
            }
            fetch_lfs(&repo, &repo.rev_parse("HEAD")?, LfsSource::Local(&endpoint))?;
        }
        Ok(())
    }

    /// Open every initialized submodule, including nested submodules.
    fn submodules(&self) -> Result<Vec<GitRepository>> {
        if !self.repo.path.join(".gitmodules").exists() {
            return Ok(Vec::new());
        }
        let submodules = GIT
            .as_ref()
            .cloned()?
            .args(&[
                "submodule",
                "foreach",
                "--quiet",
                "--recursive",
                r#"printf '%s\0' "$displaypath""#,
            ])
            .env("GIT_NO_LAZY_FETCH", "1")
            .env(EnvVars::GIT_ALLOW_PROTOCOL, "file")
            .cwd(&self.repo.path)
            .exec_with_output()?;
        submodules
            .stdout
            .split(|&byte| byte == 0)
            .filter(|path| !path.is_empty())
            .map(|path| GitRepository::open(&self.repo.path.join(paths::bytes2path(path)?)))
            .collect()
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
    fn reset(self, database: &GitDatabase, settings: &GitHttpSettings) -> Result<Self> {
        Self::invalidate_ready(&self.repo.path)?;
        let with_lfs = database.copy_lfs_to(&self)?;
        let original_remote_url = database.remote.url();

        if settings.allows_partial_fetches(original_remote_url) {
            self.repo.set_remote_url(original_remote_url)?;
        }
        // We want to skip smudge if lfs was disabled for the repository
        // as smudge filters can trigger on a reset even if lfs artifacts
        // were not originally "fetched".
        // LFS smudge requests do not honor Git's protocol restriction. Fetch local submodule
        // objects explicitly after checkout when offline.
        let lfs_skip_smudge = if with_lfs == Some(true) && !settings.offline {
            "0"
        } else {
            "1"
        };

        debug!("Reset `{}` to {}", self.repo.path.display(), self.revision);

        let mut command =
            configure_git_network(GIT.as_ref().cloned()?, original_remote_url, settings);
        if with_lfs == Some(true) && !settings.offline {
            configure_lfs_tls(&mut command, &settings.insecure_hosts);
        }
        command
            .env(EnvVars::GIT_LFS_SKIP_SMUDGE, lfs_skip_smudge)
            .cwd(&self.repo.path);

        // Perform the hard reset.
        command
            .clone()
            .arg("reset")
            .arg("--hard")
            .arg(self.revision.as_str())
            .exec_with_output()
            .map_err(|err| git_command_error(err, original_remote_url, settings.offline))?;

        // Initialize direct submodules using the original remote URL so Git can resolve relative
        // submodule URLs, but don't write it to `remote.origin.url`. Git persists resolved submodule
        // URLs during initialization, so writing a credentialed parent remote can leak credentials
        // into checkout configuration.
        //
        // Do not use `--recursive` here: command-local `remote.origin.url` config is inherited by
        // Git commands run inside submodules, which would make nested relative URLs resolve against
        // the top-level remote instead of their immediate parent submodule.
        let mut submodule_update = command.clone();
        for config in submodule_update_config(original_remote_url) {
            submodule_update.arg("-c").arg(config);
        }

        submodule_update
            .arg("submodule")
            .arg("update")
            .arg("--init")
            .exec_with_output()
            .map_err(|err| git_command_error(err, original_remote_url, settings.offline))?;

        // Recursively update nested submodules without overriding `remote.origin.url`, so each
        // nested relative URL resolves against its immediate parent submodule. The transient
        // credential rewrite is still safe to inherit because it only affects transport.
        let mut submodule_update = command;
        for config in submodule_auth_config(original_remote_url) {
            submodule_update.arg("-c").arg(config);
        }

        submodule_update
            .arg("submodule")
            .arg("update")
            .arg("--recursive")
            .arg("--init")
            .exec_with_output()
            .map_err(|err| git_command_error(err, original_remote_url, settings.offline))?;

        if with_lfs == Some(true) && settings.offline {
            self.fetch_local_submodule_lfs()?;
        }

        let lfs_validation = match with_lfs {
            None => None,
            Some(false) => Some(false),
            Some(true) => {
                Some(self.materialize_lfs()? && self.repo.lfs_fsck_objects(self.revision.as_str()))
            }
        };

        // Write the `.ok` marker after reset and LFS preparation. A failed LFS fetch or object
        // check leaves the checkout unmarked.
        if lfs_validation != Some(false) {
            paths::create(self.repo.path.with_extension(CHECKOUT_READY_EXTENSION))?;
        }

        Ok(self.with_lfs_ready(lfs_validation))
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

/// Apply network settings to Git commands.
fn configure_git_network(
    mut cmd: ProcessBuilder,
    url: &SafeUrl,
    settings: &GitHttpSettings,
) -> ProcessBuilder {
    // Terminal prompts would be hidden by the progress bar. GUI prompts remain available.
    cmd.env(EnvVars::GIT_TERMINAL_PROMPT, "0");
    if settings.disable_ssl(url) {
        let remote_url = url.without_credentials();
        let origin = remote_url_root(remote_url.clone().into_owned());
        append_git_config(&mut cmd, &format!("http.{origin}.sslVerify"), "false");
        // Git prefers URL path specificity over configuration scope.
        append_git_config(&mut cmd, &format!("http.{remote_url}.sslVerify"), "false");
        // Username matching breaks ties between equally specific paths; passwords are ignored.
        let mut username_url = url.clone();
        if !username_url.username().is_empty() && username_url.set_password(None).is_ok() {
            append_git_config(
                &mut cmd,
                &format!("http.{}.sslVerify", username_url.as_str()),
                "false",
            );
        }
    }
    if settings.offline {
        cmd.env(EnvVars::GIT_ALLOW_PROTOCOL, "file");
    }
    cmd
}

/// Converts an offline transport failure and redacts credentials from other errors.
fn git_command_error(error: anyhow::Error, url: &SafeUrl, offline: bool) -> anyhow::Error {
    let message = error.to_string();
    if offline && message.contains("transport '") && message.contains("' not allowed") {
        GitError::TransportNotAllowed.into()
    } else if url.as_str().contains('=')
        && message.contains("bogus format in GIT_CONFIG_PARAMETERS")
    {
        redact_git_error(error, url).context(
            "If Git is older than 2.31, upgrade it to use URL-specific settings containing `=`",
        )
    } else {
        redact_git_error(error, url)
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
    remote_url: &SafeUrl,
    reference: &GitReference,
    settings: &GitHttpSettings,
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
        RefspecStrategy::All => {
            fetch_with_cli(repo, remote_url, refspecs.as_slice(), tags, settings)
        }
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
                        settings,
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

/// Attempts to use `git` CLI installed on the system to fetch the given refspecs from a remote repository.
fn fetch_with_cli(
    repo: &mut GitRepository,
    url: &SafeUrl,
    refspecs: &[String],
    tags: bool,
    settings: &GitHttpSettings,
) -> Result<()> {
    let partial_fetches = settings.allows_partial_fetches(url);
    if partial_fetches {
        // A named remote lets Git record the promisor and default filter.
        repo.set_remote_url(url)?;
    }
    let mut cmd = configure_git_network(GIT.as_ref().cloned()?, url, settings);

    cmd.arg("fetch");
    if tags {
        cmd.arg("--tags");
    }
    cmd.arg("--force") // handle force pushes
        .arg("--update-head-ok"); // see discussion in #2078
    if partial_fetches {
        // Build tools can require full commit history, while trees and blobs are fetched as needed.
        cmd.arg(format!("--filter={PARTIAL_CLONE_FILTER}"))
            .arg(CHECKOUT_REMOTE);
    } else {
        cmd.arg(url.as_str());
    }
    cmd.args(refspecs).cwd(&repo.path);

    // We capture the output to avoid streaming it to the user's console during clones.
    // The required `on...line` callbacks currently do nothing.
    // The output appears to be included in error messages by default.
    cmd.exec_with_output()
        .map_err(|err| git_command_error(err, url, settings.offline))?;

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

/// Fetch one revision's trees and blobs in batches, without traversing its history.
fn fetch_checkout_objects(
    repo: &GitRepository,
    url: &SafeUrl,
    revision: &GitOid,
    settings: &GitHttpSettings,
) -> Result<()> {
    let mut requested = Vec::new();
    loop {
        let output = GIT
            .as_ref()
            .cloned()?
            .arg("rev-list")
            .arg("--objects")
            .arg("--no-object-names")
            .arg("--missing=print")
            .arg("--no-walk")
            .arg(revision.as_str())
            .env("GIT_NO_LAZY_FETCH", "1")
            .env(EnvVars::GIT_ALLOW_PROTOCOL, "file")
            .cwd(&repo.path)
            .exec_with_output()?;
        let mut missing = str::from_utf8(&output.stdout)?
            .lines()
            .filter_map(|line| line.strip_prefix('?'))
            .map(str::parse::<GitOid>)
            .collect::<Result<Vec<_>, _>>()?;
        if missing.is_empty() {
            return Ok(());
        }
        missing.sort_unstable();
        ensure!(
            missing != requested,
            "Git did not provide the objects required by {revision}"
        );

        if requested.is_empty() {
            repo.set_remote_url(url)?;
        }
        let mut fetch = configure_git_network(GIT.as_ref().cloned()?, url, settings);
        fetch
            .arg("fetch")
            .arg("--no-tags")
            .arg("--no-recurse-submodules")
            .cwd(&repo.path);
        // Avoid advertising cached commits as complete when their trees or blobs may be missing.
        let mut batch = fetch.clone();
        append_git_config(&mut batch, "fetch.negotiationAlgorithm", "noop");
        let object_ids = missing.iter().map(GitOid::as_str).collect::<Vec<_>>();
        // For a newly initialized database, request missing trees and their blobs together
        // to reduce fetch round trips.
        let filter = if repo.newly_initialized {
            "--no-filter"
        } else {
            "--filter=blob:none"
        };
        let result = batch
            .arg(filter)
            .arg("--stdin")
            .arg(CHECKOUT_REMOTE)
            .stdin(object_ids.join("\n"))
            .exec_with_output();
        if let Err(err) = result {
            if err.downcast_ref::<ProcessError>().and_then(|err| err.code) != Some(129) {
                return Err(git_command_error(err, url, settings.offline));
            }
            // Before Git 2.29, fetch accepts refspecs only as arguments. Fetch complete
            // subtrees to reduce requests. No valid ref contains "..", so the glob offers no haves.
            fetch
                .arg("--negotiation-tip=refs/*..*")
                .arg("--no-filter")
                .arg(CHECKOUT_REMOTE);
            for objects in object_ids.chunks(256) {
                fetch
                    .clone()
                    .args(objects)
                    .exec_with_output()
                    .map_err(|err| git_command_error(err, url, settings.offline))?;
            }
        }
        requested = missing;
    }
}

#[derive(Clone, Copy)]
enum LfsSource<'a> {
    Remote(&'a SafeUrl, &'a GitHttpSettings),
    Local(&'a SafeUrl),
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

    let local_settings = GitHttpSettings::default().with_offline(true);
    // Make the selected revision's Git trees and blobs available before Git LFS scans for pointers.
    if uv_preview::is_enabled(PreviewFeature::GitPartialFetches) && repo.has_promisor_remote()? {
        let (url, settings) = match source {
            LfsSource::Remote(url, settings) => (url, settings),
            LfsSource::Local(url) => (url, &local_settings),
        };
        fetch_checkout_objects(repo, url, revision, settings)?;
    }
    let endpoint;
    let (url, settings, local) = match source {
        LfsSource::Remote(url, settings) if settings.offline => {
            match repo.check_lfs_objects(revision.as_str()) {
                LfsValidation::Passed => return Ok(true),
                LfsValidation::Failed | LfsValidation::Unsupported => {}
            }
            endpoint = repo.lfs_endpoint(url.as_str())?;
            if endpoint.scheme() != "file" {
                return Err(GitError::TransportNotAllowed.into());
            }
            (&endpoint, settings, true)
        }
        LfsSource::Remote(url, settings) => (url, settings, false),
        LfsSource::Local(url) => (url, &local_settings, true),
    };
    if local {
        cmd = configure_local_lfs(cmd, repo, url)?;
    } else if !settings.insecure_hosts.is_empty() {
        configure_lfs_tls(&mut cmd, &settings.insecure_hosts);
        if !settings.insecure_hosts.contains(&TrustedHost::Wildcard) {
            let endpoint = repo.lfs_endpoint(url.as_str())?;
            cmd = configure_git_network(cmd, &endpoint, settings);
        }
    }
    let mut cmd = configure_git_network(cmd, url, settings);

    cmd.arg("lfs")
        .arg("fetch")
        .arg(url.as_str())
        .arg(revision.as_str())
        .env_remove(EnvVars::GIT_LFS_SKIP_SMUDGE)
        .cwd(&repo.path);

    cmd.exec_with_output()
        .map_err(|err| git_command_error(err, url, settings.offline))?;

    Ok(repo.lfs_fsck_objects(revision.as_str()))
}

/// Apply HTTPS exceptions for explicitly trusted LFS endpoints and object origins.
fn configure_lfs_tls(cmd: &mut ProcessBuilder, insecure_hosts: &[TrustedHost]) {
    for trusted_host in insecure_hosts {
        let (scheme, host, port) = match trusted_host {
            TrustedHost::Wildcard => {
                cmd.env(EnvVars::GIT_SSL_NO_VERIFY, "true");
                continue;
            }
            TrustedHost::Host { scheme, host, port } => (scheme, host, port),
        };
        if scheme.as_ref().is_some_and(|scheme| scheme != "https") {
            continue;
        }
        // A `*` in a host entry is not uv's wildcard, but Git LFS would interpret it as one.
        if host.contains('*') {
            continue;
        }
        // Git LFS matches an omitted port only against the default HTTPS port.
        let authority = if let Some(port) = port {
            format!("{host}:{port}")
        } else {
            host.clone()
        };
        let Ok(origin) = SafeUrl::parse(&format!("https://{authority}/")) else {
            continue;
        };
        if origin.host_str() != Some(host.as_str()) {
            continue;
        }
        append_git_config(cmd, &format!("http.{origin}.sslVerify"), "false");
    }
}

/// Force the built-in local LFS transfer, regardless of inherited endpoint or adapter settings.
fn configure_local_lfs(
    mut cmd: ProcessBuilder,
    repo: &GitRepository,
    url: &SafeUrl,
) -> Result<ProcessBuilder> {
    // Use the local path as the LFS endpoint and force the built-in file adapter.
    cmd.arg("-c").arg(format!("lfs.url={url}"));
    // The shared database may lack LFS objects from other recent revisions.
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
        Err(error) => return Err(git_command_error(error, url, true)),
    }
    Ok(cmd)
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
    #[cfg(feature = "test-git")]
    fn git_command_config_preserves_inherited_entries() -> Result<()> {
        let mut command = GIT.as_ref().cloned()?;
        command
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env(
                "GIT_CONFIG_GLOBAL",
                if cfg!(windows) { "NUL" } else { "/dev/null" },
            )
            .env("GIT_CONFIG_COUNT", "0")
            .env(
                "GIT_CONFIG_PARAMETERS",
                "'test.parameter=inherited parameter value'",
            );
        let value = "spaces, apostrophes ' and exclamations ! with equals =";
        append_git_config(&mut command, "test.value", value);
        let inherited = command
            .clone()
            .args(&["config", "--get", "test.parameter"])
            .exec_with_output()?;
        assert_eq!(
            str::from_utf8(&inherited.stdout)?.trim(),
            "inherited parameter value"
        );
        let escaped = command
            .clone()
            .args(&["config", "--get", "test.value"])
            .exec_with_output()?;
        assert_eq!(str::from_utf8(&escaped.stdout)?.trim(), value);
        // Split keys and counted configuration require Git 2.31.
        let split_support = command
            .clone()
            .env("GIT_CONFIG_PARAMETERS", "'test.split'='true'")
            .args(&["config", "--get", "test.split"])
            .exec_with_output();
        if let Err(error) = split_support {
            if error
                .to_string()
                .contains("bogus format in GIT_CONFIG_PARAMETERS")
            {
                return Ok(());
            }
            return Err(error);
        }
        command
            .env("GIT_CONFIG_COUNT", "1")
            .env("GIT_CONFIG_KEY_0", "test.counted")
            .env("GIT_CONFIG_VALUE_0", "inherited counted value");
        append_git_config(
            &mut command,
            "http.https://example.invalid/a=b.sslVerify",
            "false",
        );
        let inherited = command
            .clone()
            .args(&["config", "--get", "test.counted"])
            .exec_with_output()?;
        assert_eq!(
            str::from_utf8(&inherited.stdout)?.trim(),
            "inherited counted value"
        );
        let matching = command
            .args(&[
                "config",
                "--get-urlmatch",
                "http.sslVerify",
                "https://example.invalid/a=b",
            ])
            .exec_with_output()?;
        assert_eq!(str::from_utf8(&matching.stdout)?.trim(), "false");
        Ok(())
    }

    #[cfg(feature = "test-git")]
    fn git_tls_command() -> Result<ProcessBuilder> {
        let mut command = GIT.as_ref().cloned()?;
        command
            .args(&["-c", "http.sslVerify=true"])
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env(
                "GIT_CONFIG_GLOBAL",
                if cfg!(windows) { "NUL" } else { "/dev/null" },
            )
            .env("GIT_CONFIG_COUNT", "0")
            .env_remove("GIT_CONFIG_PARAMETERS")
            .env_remove(EnvVars::GIT_SSL_NO_VERIFY);
        Ok(command)
    }

    #[test]
    #[cfg(feature = "test-git")]
    fn git_tls_exception_is_scoped_to_remote_host() -> Result<()> {
        let url = SafeUrl::parse("https://user:password@allowed.example/org/repo.git")?;
        let settings =
            GitHttpSettings::default().with_insecure_hosts(vec!["allowed.example".parse()?]);
        let mut command = git_tls_command()?;
        command
            .args(&["-c", "http.https://allowed.example/org/.sslVerify=true"])
            .env("GIT_CONFIG_COUNT", "1")
            .env(
                "GIT_CONFIG_KEY_0",
                "http.https://user@allowed.example/org/repo.git.sslVerify",
            )
            .env("GIT_CONFIG_VALUE_0", "true");
        let command = configure_git_network(command, &url, &settings);

        for (url, expected) in [
            (url.as_str(), "false"),
            ("https://allowed.example/org/repo.git", "false"),
            ("https://allowed.example/other.git", "false"),
            ("https://untrusted.example/submodule.git", "true"),
        ] {
            let output = command
                .clone()
                .args(&["config", "--get-urlmatch", "http.sslVerify", url])
                .exec_with_output()?;
            assert_eq!(str::from_utf8(&output.stdout)?.trim(), expected, "{url}");
        }
        Ok(())
    }

    #[test]
    #[cfg(feature = "test-git")]
    fn lfs_tls_exception_is_scoped_to_object_origin() -> Result<()> {
        let mut command = git_tls_command()?;
        configure_lfs_tls(
            &mut command,
            &[
                "https://objects.example:9443".parse()?,
                "default.example".parse()?,
            ],
        );

        for (url, expected) in [
            ("https://objects.example:9443/file", "false"),
            ("https://objects.example:8443/file", "true"),
            ("http://objects.example:9443/file", "true"),
            ("https://default.example/file", "false"),
            ("https://default.example:9443/file", "true"),
            ("https://other.example:9443/file", "true"),
        ] {
            let output = command
                .clone()
                .args(&["config", "--get-urlmatch", "http.sslVerify", url])
                .exec_with_output()?;
            assert_eq!(str::from_utf8(&output.stdout)?.trim(), expected, "{url}");
        }
        Ok(())
    }

    #[test]
    #[cfg(feature = "test-git")]
    fn lfs_tls_exception_does_not_expand_literal_hosts() -> Result<()> {
        let mut command = git_tls_command()?;
        configure_lfs_tls(
            &mut command,
            &["*.example", "user@objects.example", "objects.example/path"].map(|host| {
                TrustedHost::Host {
                    scheme: None,
                    host: host.to_owned(),
                    port: None,
                }
            }),
        );
        let literal = command
            .args(&[
                "config",
                "--get-urlmatch",
                "http.sslVerify",
                "https://user@objects.example/path/file",
            ])
            .exec_with_output()?;
        assert_eq!(str::from_utf8(&literal.stdout)?.trim(), "true");
        Ok(())
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
