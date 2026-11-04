//! Git support is derived from Cargo's implementation.
//! Cargo is dual-licensed under either Apache 2.0 or MIT, at the user's choice.
//! Source: <https://github.com/rust-lang/cargo/blob/23eb492cf920ce051abfc56bbaf838514dc8365c/src/cargo/sources/git/utils.rs>
use std::path::{Path, PathBuf, absolute};
use std::str::{self};
use std::sync::LazyLock;

use anyhow::{Context, Result, anyhow};
use cargo_util::{ProcessBuilder, ProcessError, paths};
use owo_colors::OwoColorize;
use percent_encoding::{CONTROLS, percent_encode};
use tracing::{debug, instrument, warn};

use uv_auth::TrustedHost;
use uv_fs::Simplified;
use uv_git_types::{GitOid, GitReference};
use uv_redacted::SafeUrl;
use uv_static::EnvVars;
use uv_warnings::warn_user_once;

use crate::GitHttpSettings;

/// Extension for the marker beside a completed checkout.
/// See [`GitCheckout::reset`] for why we need this.
const CHECKOUT_READY_EXTENSION: &str = "ok";
/// Written after LFS objects have been verified and files materialized in the root and initialized submodules.
const LFS_READY: &[u8] = b"lfs-v2";
/// Written after materialization when Git LFS lacks `fsck --objects`.
const LFS_UNCHECKED: &[u8] = b"lfs-v2-unchecked";
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
    /// Transport settings for this database, including any trusted mirror.
    settings: GitHttpSettings,
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
    /// Worktree containing the selected revision's `.lfsconfig` for Git LFS commands.
    lfs_worktree: Option<tempfile::TempDir>,
}

/// The most severe validation outcome determines the marker for the entire checkout.
#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum LfsValidation {
    Passed,
    Unsupported,
    Failed,
}

impl GitRepository {
    fn at(path: &Path) -> Self {
        Self {
            path: path.to_path_buf(),
            lfs_worktree: None,
        }
    }

    /// Opens an existing Git repository at `path`.
    fn open(path: &Path) -> Result<Self> {
        // Restrict discovery to this directory because an empty cache can be inside another repository.
        GIT.as_ref()
            .cloned()?
            .args(&["--git-dir=.git", "rev-parse"])
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

        Ok(Self::at(path))
    }

    /// Provide the revision's `.lfsconfig` to Git LFS in a temporary worktree.
    fn for_lfs_revision(&self, revision: &GitOid) -> Result<Self> {
        let object = format!("{revision}:.lfsconfig");
        let output = GIT
            .as_ref()
            .cloned()?
            .args(&["cat-file", "--batch=%(objecttype)"])
            .stdin(format!("{object}\n"))
            .env("GIT_NO_LAZY_FETCH", "1")
            .env(EnvVars::GIT_ALLOW_PROTOCOL, "file")
            .cwd(&self.path)
            .exec_with_output()?;
        let response = output
            .stdout
            .strip_suffix(b"\n")
            .context("unexpected git cat-file response")?;
        let contents = if let Some(contents) = response.strip_prefix(b"blob\n") {
            contents
        } else if response == format!("{object} missing").as_bytes() {
            // An empty file stops Git LFS from falling back to a different revision at HEAD.
            &[]
        } else {
            return Err(anyhow!("expected a file at `{object}`"));
        };
        let worktree = tempfile::tempdir_in(self.path.join(".git"))?;
        fs_err::write(worktree.path().join(".lfsconfig"), contents)?;
        Ok(Self {
            lfs_worktree: Some(worktree),
            ..Self::at(&self.path)
        })
    }

    fn lfs_command(&self, mut cmd: ProcessBuilder) -> ProcessBuilder {
        if let Some(worktree) = &self.lfs_worktree {
            cmd.env(EnvVars::GIT_DIR, self.path.join(".git"))
                .env(EnvVars::GIT_WORK_TREE, worktree.path());
        }
        cmd.cwd(&self.path);
        cmd
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

    /// Resolve the LFS endpoint using Git LFS's configuration and URL rewriting rules.
    fn lfs_endpoint(&self, remote: &str) -> Result<SafeUrl> {
        let mut cmd = self.lfs_command(GIT.as_ref().cloned()?);
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
            .exec_with_output()
            // The remaining output can contain credentials and arbitrary environment values.
            .map_err(|_| anyhow!("failed to determine Git LFS endpoint"))?;
        // `git lfs env` can print a non-UTF-8 storage path alongside the HTTPS endpoint.
        let endpoint = output
            .stdout
            .split(|&byte| byte == b'\n')
            .find_map(|line| line.strip_prefix(prefix.as_bytes()))
            .and_then(|line| {
                let suffix = b" (auth=";
                let end = line
                    .windows(suffix.len())
                    .rposition(|bytes| bytes == suffix)?;
                Some(&line[..end])
            })
            .context("Git LFS did not report its endpoint")?;
        Ok(SafeUrl::parse(str::from_utf8(endpoint)?)?)
    }

    /// Get the remote used by a detached submodule when an LFS smudge filter is configured.
    fn lfs_smudge_remote(&self) -> Result<Option<String>> {
        let config = GIT
            .as_ref()
            .cloned()?
            .args(&[
                "config",
                "--null",
                "--get-regexp",
                r"^(filter\.lfs\.(process|smudge)|remote\.lfsdefault)$",
            ])
            .cwd(&self.path)
            .exec_with_output();
        match config {
            Ok(output) => {
                let mut remote = "origin";
                let mut process = false;
                let mut smudge = false;
                for entry in output.stdout.split(|&byte| byte == 0) {
                    let mut fields = entry.splitn(2, |&byte| byte == b'\n');
                    let key = fields.next().unwrap_or_default();
                    let value = fields.next().unwrap_or_default();
                    let enabled = !value.trim_ascii().is_empty();
                    match key {
                        b"filter.lfs.process" => process = enabled,
                        b"filter.lfs.smudge" => smudge = enabled,
                        b"remote.lfsdefault" => remote = str::from_utf8(value)?,
                        _ => {}
                    }
                }
                Ok((process || smudge).then(|| remote.to_owned()))
            }
            Err(error)
                if error
                    .downcast_ref::<ProcessError>()
                    .is_some_and(|error| error.code == Some(1)) =>
            {
                Ok(None)
            }
            Err(error) => Err(error),
        }
    }

    fn materialize_submodule_lfs(&self, offline: bool) -> Result<LfsValidation> {
        let mut validation = self.check_lfs_objects("HEAD");
        if offline
            && self.needs_lfs_fetch("HEAD", &validation)?
            && let Some(remote) = self.lfs_smudge_remote()?
        {
            let endpoint = self.lfs_endpoint(&remote)?;
            if endpoint.scheme() != "file" {
                return Err(GitError::TransportNotAllowed.into());
            }
            fetch_lfs(self, &self.rev_parse("HEAD")?, LfsSource::Local(&endpoint))?;
            validation = self.check_lfs_objects("HEAD");
        }
        if validation != LfsValidation::Failed {
            self.materialize_lfs()?;
        }
        Ok(validation)
    }

    /// Retry submodule updates with TLS exceptions for matching effective URLs.
    fn retry_submodules_with_tls(
        &self,
        command: &ProcessBuilder,
        settings: &GitHttpSettings,
    ) -> Result<()> {
        let mut pending = vec![Self::at(&self.path)];
        while let Some(repo) = pending.pop() {
            if !repo.path.join(".gitmodules").exists() {
                continue;
            }
            let mut update = command.clone();
            update.cwd(&repo.path);
            // The root's direct submodule URLs were initialized against its original remote.
            // Nested submodule URLs resolve against their parent repository's origin.
            if repo.path != self.path {
                update
                    .clone()
                    .args(&["submodule", "init"])
                    .exec_with_output()?;
            }
            let config = update
                .clone()
                .args(&["config", "--null", "--list"])
                .exec_with_output()?;
            for entry in config.stdout.split(|&byte| byte == 0) {
                let mut entry = entry.splitn(2, |&byte| byte == b'\n');
                let key = entry.next().unwrap_or_default();
                if key.starts_with(b"submodule.")
                    && key.ends_with(b".url")
                    && let Some(remote) = entry.next()
                    && let Some(url) = expanded_git_url(update.clone(), paths::bytes2path(remote)?)?
                    && url.scheme() == "https"
                    && settings.disable_ssl(&url)
                    && !url.host_str().is_some_and(|host| host.contains('*'))
                {
                    // A more-specific inherited setting takes precedence over the origin alone.
                    configure_url_tls(&mut update, &url);
                }
            }
            // A failed smudge can advance the submodule's `HEAD` while leaving files missing.
            // Force checkout so Git retries even when `HEAD` matches the recorded revision.
            update
                .args(&["submodule", "update", "--init", "--force"])
                .exec_with_output()?;
            pending.extend(repo.submodules()?);
        }
        Ok(())
    }

    /// Open the initialized direct submodules of this working tree.
    fn submodules(&self) -> Result<Vec<Self>> {
        if !self.path.join(".gitmodules").exists() {
            return Ok(Vec::new());
        }
        let submodules = GIT
            .as_ref()
            .cloned()?
            .args(&[
                "submodule",
                "foreach",
                "--quiet",
                r#"printf '%s\0' "$displaypath""#,
            ])
            .env("GIT_NO_LAZY_FETCH", "1")
            .env(EnvVars::GIT_ALLOW_PROTOCOL, "file")
            .cwd(&self.path)
            .exec_with_output()?;
        submodules
            .stdout
            .split(|&byte| byte == 0)
            .filter(|path| !path.is_empty())
            .map(|path| Self::open(&self.path.join(paths::bytes2path(path)?)))
            .collect()
    }

    /// Return whether a revision needs an offline LFS transfer.
    fn needs_lfs_fetch(&self, refname: &str, validation: &LfsValidation) -> Result<bool> {
        match validation {
            LfsValidation::Passed => Ok(false),
            LfsValidation::Failed => Ok(true),
            // A local fetch honors the path filters and needs no transfer when all selected objects exist.
            LfsValidation::Unsupported => Ok(!self.has_selected_lfs_objects(refname)?),
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

        let mut git = self.lfs_command(GIT.as_ref().cloned()?);
        git.env("GIT_NO_LAZY_FETCH", "1")
            .env(EnvVars::GIT_ALLOW_PROTOCOL, "file");
        let env = git.clone().args(&["lfs", "env"]).exec_with_output()?;
        if !env.stdout.split(|&byte| byte == b'\n').any(|line| {
            line.strip_prefix(b"FetchInclude=")
                .is_some_and(|value| !value.trim_ascii().is_empty())
        }) {
            return Ok(false);
        }

        self.has_selected_lfs_objects(refname)
    }

    /// Check whether this repository can supply the revision's selected LFS files locally.
    fn has_selected_lfs_objects(&self, refname: &str) -> Result<bool> {
        let url = self.file_url()?;
        let cmd = self.lfs_command(GIT.as_ref().cloned()?);
        Ok(configure_local_lfs(cmd, self, &url)?
            .args(&["lfs", "fetch", url.as_str(), refname])
            .env("GIT_NO_LAZY_FETCH", "1")
            .env(EnvVars::GIT_ALLOW_PROTOCOL, "file")
            .exec_with_output()
            .is_ok())
    }

    /// Check LFS objects, distinguishing an unsupported `fsck --objects` from failure.
    #[instrument(skip_all, fields(path = %self.path.user_display(), refname = %refname))]
    fn check_lfs_objects(&self, refname: &str) -> LfsValidation {
        let mut cmd = if let Ok(lfs) = GIT_LFS.as_ref() {
            self.lfs_command(lfs.clone())
        } else {
            warn!("Git LFS is not available, skipping LFS fetch");
            return LfsValidation::Failed;
        };

        // Requires Git LFS 3.x (2021 release)
        let result = cmd
            .arg("fsck")
            .arg("--objects")
            .arg(refname)
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
        settings: &GitHttpSettings,
        with_lfs: bool,
    ) -> Result<(GitDatabase, GitOid)> {
        let locked_reference = locked_rev.map(GitReference::Commit);
        let reference = locked_reference.as_ref().unwrap_or(reference);
        if let Some(mut db) = db {
            fetch(&mut db.repo, &self.url, reference, &db.settings)
                .with_context(|| format!("failed to fetch into: {}", into.user_display()))?;

            if let Ok(rev) = db.repo.resolve(reference) {
                if with_lfs {
                    let lfs_ready =
                        fetch_lfs(&db.repo, &rev, LfsSource::Remote(&self.url, &db.settings))
                            .with_context(|| format!("failed to fetch LFS objects at {rev}"))?;
                    db = db.with_lfs_ready(Some(lfs_ready));
                }
                return Ok((db, rev));
            }
        }

        let mut db = self.init_db(into, settings)?;
        fetch(&mut db.repo, &self.url, reference, &db.settings)
            .with_context(|| format!("failed to clone into: {}", into.user_display()))?;
        let rev = db.repo.resolve(reference)?;
        if with_lfs {
            db.lfs_ready = Some(
                fetch_lfs(&db.repo, &rev, LfsSource::Remote(&self.url, &db.settings))
                    .with_context(|| format!("failed to fetch LFS objects at {rev}"))?,
            );
        }
        Ok((db, rev))
    }

    /// Open the cache database when possible. Mirror configuration errors propagate.
    pub(crate) fn db_at(
        &self,
        path: &Path,
        settings: &GitHttpSettings,
    ) -> Result<Option<GitDatabase>> {
        GitRepository::open(path)
            .ok()
            .map(|repo| self.database(repo, settings))
            .transpose()
    }

    /// Initialize the cache database, replacing any existing database, and resolve its transport settings.
    fn init_db(&self, path: &Path, settings: &GitHttpSettings) -> Result<GitDatabase> {
        if settings.has_named_tls_exceptions() && path.join(".git").exists() {
            // Ignore the existing database's config while retaining its Git directory for `includeIf`.
            // If user config also prevents URL expansion, keep the existing database intact.
            let empty = tempfile::tempdir_in(path)?;
            let mut probe = GIT.as_ref().cloned()?;
            probe
                .arg("--git-dir=.git")
                .cwd(path)
                .env(EnvVars::GIT_COMMON_DIR, empty.path());
            expanded_git_url(probe, self.url.as_str())?;
        }
        match fs_err::remove_dir_all(path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        fs_err::create_dir_all(path)?;
        self.database(GitRepository::init(path)?, settings)
    }

    fn database(&self, repo: GitRepository, settings: &GitHttpSettings) -> Result<GitDatabase> {
        let settings = settings.clone().for_remote(&self.url, &repo.path)?;
        Ok(GitDatabase {
            remote: self.clone(),
            repo,
            settings,
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
        // Git LFS can read `.lfsconfig` from `HEAD` while the hard reset populates the worktree.
        GIT.as_ref()
            .cloned()?
            .args(&["update-ref", "HEAD", checkout.revision.as_str()])
            .cwd(&checkout.repo.path)
            .exec_with_output()?;
        let url = self.repo.file_url()?;
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
        let Ok(repo) = self.repo.for_lfs_revision(&oid) else {
            return false;
        };
        let refname = format!("{oid}^0");
        matches!(
            repo.needs_lfs_fetch(&refname, &repo.check_lfs_objects(&refname)),
            Ok(false)
        )
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
        // Defer checkout until reset applies the requested transport settings.
        clone.args(&["clone", "--no-checkout"]);
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
                // Older markers may cover only the root, and the worktree may still contain LFS pointers.
                Self::invalidate_ready(&self.repo.path)?;
                self.repo.materialize_lfs()?;
                self.finish_lfs(validation, None)
            }
        }
    }

    /// Materialize and check submodules before marking the checkout ready.
    /// Offline updates materialize each `.gitmodules` before initializing its children.
    fn finish_lfs(
        self,
        mut validation: LfsValidation,
        offline_update: Option<(&ProcessBuilder, &SafeUrl)>,
    ) -> Result<Self> {
        let children = |repo: &GitRepository| {
            if let Some((command, url)) = offline_update {
                command
                    .clone()
                    .args(&["submodule", "update", "--init"])
                    .cwd(&repo.path)
                    .exec_with_output()
                    .map_err(|err| git_command_error(err, url, true))?;
            }
            repo.submodules()
        };
        let mut pending = children(&self.repo)?;
        while let Some(repo) = pending.pop() {
            validation = validation.max(repo.materialize_submodule_lfs(offline_update.is_some())?);
            if validation == LfsValidation::Failed {
                break;
            }
            pending.extend(children(&repo)?);
        }
        self.finish(Some(validation))
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
        let settings = &database.settings;

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
        command
            .env(EnvVars::GIT_LFS_SKIP_SMUDGE, lfs_skip_smudge)
            .cwd(&self.repo.path);

        let mut reset = command.clone();
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
            .exec_with_output()
            .map_err(|err| git_command_error(err, original_remote_url, settings.offline))?;

        // Resolve relative submodule URLs against the original parent remote. Git records the
        // resolved URLs in the checkout configuration, so the parent URL must be credential-free.
        let (origin, auth_rewrite) = submodule_config(original_remote_url);
        if self.repo.path.join(".gitmodules").exists() {
            command
                .clone()
                .arg("-c")
                .arg(origin)
                .args(&["submodule", "init"])
                .exec_with_output()
                .map_err(|err| git_command_error(err, original_remote_url, settings.offline))?;
        }

        // When a submodule has no LFS endpoint configured, its LFS downloads use its own origin.
        // Relative URLs in nested submodules also resolve against that origin.
        if let Some(auth_rewrite) = auth_rewrite {
            command.arg("-c").arg(auth_rewrite);
        }
        if with_lfs == Some(true) && settings.offline {
            let validation = self.repo.check_lfs_objects(self.revision.as_str());
            return self.finish_lfs(validation, Some((&command, original_remote_url)));
        }
        if let Err(err) = command
            .clone()
            .args(&["submodule", "update", "--recursive", "--init"])
            .exec_with_output()
        {
            if settings.offline
                || !settings.has_named_tls_exceptions()
                || !self.repo.path.join(".gitmodules").exists()
            {
                return Err(git_command_error(
                    err,
                    original_remote_url,
                    settings.offline,
                ));
            }
            // A child can have a path-specific TLS setting that overrides the named origin.
            self.repo
                .retry_submodules_with_tls(&command, settings)
                .map_err(|err| git_command_error(err, original_remote_url, false))?;
        }

        match with_lfs {
            None => self.finish(None),
            Some(false) => Ok(self.with_lfs_ready(Some(false))),
            Some(true) => {
                let validation = self.repo.check_lfs_objects(self.revision.as_str());
                self.finish_lfs(validation, None)
            }
        }
    }
}

/// Build the origin override for `git submodule init` and the optional credential rewrite for
/// `git submodule update`.
fn submodule_config(original_remote_url: &SafeUrl) -> (String, Option<String>) {
    let remote_url = original_remote_url.without_credentials();
    let origin = format!("remote.origin.url={}", remote_url.as_str());
    if remote_url.as_str() == original_remote_url.as_str() {
        return (origin, None);
    }

    let safe_root = remote_url_root(remote_url.into_owned());
    let credentialed_root = remote_url_root(original_remote_url.clone());
    let auth_rewrite = (safe_root.as_str() != credentialed_root.as_str()).then(|| {
        format!(
            "url.{}.insteadOf={}",
            credentialed_root.as_str(),
            safe_root.as_str()
        )
    });
    (origin, auth_rewrite)
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

/// Expand Git's URL rewrites locally. An HTTPS path can contain non-UTF-8 bytes.
fn expanded_git_url(
    mut command: ProcessBuilder,
    remote: impl AsRef<std::ffi::OsStr>,
) -> Result<Option<SafeUrl>> {
    let output = command
        .args(&["ls-remote", "--get-url", "--"])
        .arg(remote.as_ref())
        .exec_with_output()
        // The source and expanded URL can both contain credentials.
        .map_err(|_| anyhow!("failed to determine Git remote URL"))?;
    let expanded = output.stdout.strip_suffix(b"\n").unwrap_or(&output.stdout);
    let expanded = expanded.strip_suffix(b"\r").unwrap_or(expanded);
    Ok(SafeUrl::parse(&percent_encode(expanded, CONTROLS).to_string()).ok())
}

impl GitHttpSettings {
    fn has_named_tls_exceptions(&self) -> bool {
        !self.insecure_hosts.is_empty() && !self.insecure_hosts.contains(&TrustedHost::Wildcard)
    }

    /// Resolve Git's URL rewrite locally when a named TLS exception might apply to the mirror.
    fn for_remote(mut self, url: &SafeUrl, database: &Path) -> Result<Self> {
        if !self.has_named_tls_exceptions() {
            return Ok(self);
        }
        let mut command = GIT.as_ref().cloned()?;
        command.arg("--git-dir=.git").cwd(database);
        let mirror = expanded_git_url(command, url.as_str())?;
        self.trusted_mirror = mirror.filter(|mirror| {
            mirror != url && mirror.scheme() == "https" && self.disable_ssl(mirror)
        });
        Ok(self)
    }
}

/// Apply network settings to Git commands.
fn configure_git_network(
    mut cmd: ProcessBuilder,
    url: &SafeUrl,
    settings: &GitHttpSettings,
) -> ProcessBuilder {
    // Terminal prompts would be hidden by the progress bar. GUI prompts remain available.
    cmd.env(EnvVars::GIT_TERMINAL_PROMPT, "0");
    if settings.insecure_hosts.contains(&TrustedHost::Wildcard) {
        // Git can contact a different HTTPS origin after a `url.*.insteadOf` rewrite.
        cmd.env(EnvVars::GIT_SSL_NO_VERIFY, "true");
    } else {
        configure_https_origins(&mut cmd, &settings.insecure_hosts);
        if settings.disable_ssl(url) {
            configure_url_tls(&mut cmd, url);
        }
        if let Some(mirror) = &settings.trusted_mirror {
            configure_url_tls(&mut cmd, mirror);
        }
    }
    if settings.offline {
        cmd.env(EnvVars::GIT_ALLOW_PROTOCOL, "file");
    }
    cmd
}

fn configure_url_tls(cmd: &mut ProcessBuilder, url: &SafeUrl) {
    let remote_url = url.without_credentials();
    let origin = remote_url_root(remote_url.clone().into_owned());
    append_git_config(cmd, &format!("http.{origin}.sslVerify"), "false");
    // Git prefers URL path specificity over configuration scope.
    append_git_config(cmd, &format!("http.{remote_url}.sslVerify"), "false");
    // Git uses the username to break ties between equally specific paths.
    let mut username_url = url.clone();
    if !username_url.username().is_empty() && username_url.set_password(None).is_ok() {
        append_git_config(
            cmd,
            &format!("http.{}.sslVerify", username_url.as_str()),
            "false",
        );
    }
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
    let mut cmd = configure_git_network(GIT.as_ref().cloned()?, url, settings);

    cmd.arg("fetch");
    if tags {
        cmd.arg("--tags");
    }
    cmd.arg("--force") // handle force pushes
        .arg("--update-head-ok") // see discussion in #2078
        .arg(url.as_str())
        .args(refspecs)
        .cwd(&repo.path);

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
    let config_repo = match source {
        LfsSource::Remote(..) => Some(repo.for_lfs_revision(revision)?),
        LfsSource::Local(_) => None,
    };
    let repo = config_repo.as_ref().unwrap_or(repo);
    cmd = repo.lfs_command(cmd);
    let endpoint;
    let (url, settings, local) = match source {
        LfsSource::Remote(url, settings) if settings.offline => {
            let refname = revision.as_str();
            let validation = repo.check_lfs_objects(refname);
            if !repo.needs_lfs_fetch(refname, &validation)? {
                return Ok(true);
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
    } else if settings.has_named_tls_exceptions() {
        let endpoint = repo.lfs_endpoint(url.as_str())?;
        if settings.disable_ssl(&endpoint) {
            configure_url_tls(&mut cmd, &endpoint);
        }
    }
    let mut cmd = configure_git_network(cmd, url, settings);

    cmd.arg("lfs")
        .arg("fetch")
        .arg(url.as_str())
        .arg(revision.as_str())
        .env_remove(EnvVars::GIT_LFS_SKIP_SMUDGE);

    cmd.exec_with_output()
        .map_err(|err| git_command_error(err, url, settings.offline))?;

    match source {
        LfsSource::Remote(..) => {
            Ok(repo.check_lfs_objects(revision.as_str()) != LfsValidation::Failed)
        }
        // The checkout verifies its local copies after resetting to this revision.
        LfsSource::Local(_) => Ok(true),
    }
}

/// Apply HTTPS exceptions to named origins contacted by Git and Git LFS subprocesses.
fn configure_https_origins(cmd: &mut ProcessBuilder, insecure_hosts: &[TrustedHost]) {
    for trusted_host in insecure_hosts {
        let TrustedHost::Host { scheme, host, port } = trusted_host else {
            continue;
        };
        if scheme.as_ref().is_some_and(|scheme| scheme != "https") {
            continue;
        }
        // Git treats `*` in a hostname as a pattern. uv matches named hosts literally.
        if host.contains('*') {
            continue;
        }
        // Git matches an omitted port only against the default HTTPS port.
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
    #[cfg(feature = "test-git")]
    use std::io::Write;

    use super::*;

    #[test]
    fn submodule_config_strips_credentials_from_origin_override() {
        let url = SafeUrl::parse("https://user:password@example.com/org/repo.git").unwrap();

        assert_eq!(
            submodule_config(&url),
            (
                "remote.origin.url=https://example.com/org/repo.git".to_string(),
                Some(
                    "url.https://user:password@example.com/.insteadOf=https://example.com/"
                        .to_string()
                ),
            )
        );
    }

    #[test]
    fn submodule_config_preserves_git_ssh_user() {
        let url = SafeUrl::parse("ssh://git@example.com/org/repo.git").unwrap();

        assert_eq!(
            submodule_config(&url),
            (
                "remote.origin.url=ssh://git@example.com/org/repo.git".to_string(),
                None
            )
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
        assert!(command.get_env(EnvVars::GIT_SSL_NO_VERIFY).is_none());

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
    fn git_tls_exception_applies_to_named_mirror() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let mut git = git_tls_command()?;
        git.cwd(directory.path());
        git.clone()
            .args(&["init", "--template="])
            .exec_with_output()?;
        // Git permits non-UTF-8 bytes in a mirror's path.
        fs_err::OpenOptions::new()
            .append(true)
            .open(directory.path().join(".git/config"))?
            .write_all(b"\n[url \"https://mirror.example:9443/\xff/\"]\n\tinsteadOf = https://source.example/\n")?;
        let source = SafeUrl::parse("https://source.example/project.git")?;
        let settings = GitHttpSettings::default()
            .with_insecure_hosts(vec!["mirror.example".parse()?])
            .for_remote(&source, directory.path())?;

        // An unqualified host also matches a rewritten URL on a non-default port.
        let command = configure_git_network(git, &source, &settings);
        for (url, expected) in [
            ("https://mirror.example:9443/%FF/project.git", "false"),
            (source.as_str(), "true"),
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
    fn git_tls_wildcard_applies_to_rewritten_origins() -> Result<()> {
        let url = SafeUrl::parse("https://source.example/repo.git")?;
        let settings = GitHttpSettings::default().with_insecure_hosts(vec![TrustedHost::Wildcard]);
        let command = configure_git_network(git_tls_command()?, &url, &settings);
        assert_eq!(
            command.get_env(EnvVars::GIT_SSL_NO_VERIFY),
            Some("true".into())
        );
        Ok(())
    }

    #[test]
    #[cfg(feature = "test-git")]
    fn lfs_tls_exception_is_scoped_to_object_origin() -> Result<()> {
        let mut command = git_tls_command()?;
        configure_https_origins(
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
        configure_https_origins(
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
