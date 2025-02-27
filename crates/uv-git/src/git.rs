//! Git support is derived from Cargo's implementation.
//! Cargo is dual-licensed under either Apache 2.0 or MIT, at the user's choice.
//! Source: <https://github.com/rust-lang/cargo/blob/23eb492cf920ce051abfc56bbaf838514dc8365c/src/cargo/sources/git/utils.rs>
use std::env;
use std::fmt::Display;
use std::path::{Path, PathBuf};
use std::str::{self};
use std::sync::LazyLock;

use anyhow::{Context, Result};
use cargo_util::{paths, ProcessBuilder};
use reqwest::StatusCode;
use reqwest_middleware::ClientWithMiddleware;
use tracing::{debug, warn};
use url::Url;

use uv_fs::Simplified;
use uv_git_types::{GitHubRepository, GitOid, GitReference};
use uv_static::EnvVars;
use uv_version::version;

/// A file indicates that if present, `git reset` has been done and a repo
/// checkout is ready to go. See [`GitCheckout::reset`] for why we need this.
const CHECKOUT_READY_LOCK: &str = ".ok";

#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error("Git executable not found. Ensure that Git is installed and available.")]
    GitNotFound,
    #[error(transparent)]
    Other(#[from] which::Error),
}

/// A global cache of the result of `which git`.
pub static GIT: LazyLock<Result<PathBuf, GitError>> = LazyLock::new(|| {
    which::which("git").map_err(|e| match e {
        which::Error::CannotFindBinaryPath => GitError::GitNotFound,
        e => GitError::Other(e),
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
    url: Url,
}

/// A local clone of a remote repository's database. Multiple [`GitCheckout`]s
/// can be cloned from a single [`GitDatabase`].
pub(crate) struct GitDatabase {
    /// Underlying Git repository instance for this database.
    repo: GitRepository,
}

/// A local checkout of a particular revision from a [`GitRepository`].
pub(crate) struct GitCheckout {
    /// The git revision this checkout is for.
    revision: GitOid,
    /// Underlying Git repository instance for this checkout.
    repo: GitRepository,
}

/// A local Git repository.
pub(crate) struct GitRepository {
    /// Path to the underlying Git repository on the local filesystem.
    path: PathBuf,
}

impl GitRepository {
    /// Opens an existing Git repository at `path`.
    pub(crate) fn open(path: &Path) -> Result<GitRepository> {
        // Make sure there is a Git repository at the specified path.
        ProcessBuilder::new(GIT.as_ref()?)
            .arg("rev-parse")
            .cwd(path)
            .exec_with_output()?;

        Ok(GitRepository {
            path: path.to_path_buf(),
        })
    }

    /// Initializes a Git repository at `path`.
    fn init(path: &Path) -> Result<GitRepository> {
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

        Ok(GitRepository {
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
}

impl GitRemote {
    /// Creates an instance for a remote repository URL.
    pub(crate) fn new(url: &Url) -> Self {
        Self { url: url.clone() }
    }

    /// Gets the remote repository URL.
    pub(crate) fn url(&self) -> &Url {
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
        client: &ClientWithMiddleware,
        disable_ssl: bool,
    ) -> Result<(GitDatabase, GitOid)> {
        let reference = locked_rev
            .map(ReferenceOrOid::Oid)
            .unwrap_or(ReferenceOrOid::Reference(reference));
        let enable_lfs_fetch = env::var(EnvVars::UV_GIT_LFS).is_ok();

        if let Some(mut db) = db {
            fetch(&mut db.repo, &self.url, reference, client, disable_ssl)
                .with_context(|| format!("failed to fetch into: {}", into.user_display()))?;

            let resolved_commit_hash = match locked_rev {
                Some(rev) => db.contains(rev).then_some(rev),
                None => reference.resolve(&db.repo).ok(),
            };

            if let Some(rev) = resolved_commit_hash {
                if enable_lfs_fetch {
                    fetch_lfs(&mut db.repo, &self.url, &rev, disable_ssl)
                        .with_context(|| format!("failed to fetch LFS objects at {rev}"))?;
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
        fetch(&mut repo, &self.url, reference, client, disable_ssl)
            .with_context(|| format!("failed to clone into: {}", into.user_display()))?;
        let rev = match locked_rev {
            Some(rev) => rev,
            None => reference.resolve(&repo)?,
        };
        if enable_lfs_fetch {
            fetch_lfs(&mut repo, &self.url, &rev, disable_ssl)
                .with_context(|| format!("failed to fetch LFS objects at {rev}"))?;
        }

        Ok((GitDatabase { repo }, rev))
    }

    /// Creates a [`GitDatabase`] of this remote at `db_path`.
    #[allow(clippy::unused_self)]
    pub(crate) fn db_at(&self, db_path: &Path) -> Result<GitDatabase> {
        let repo = GitRepository::open(db_path)?;
        Ok(GitDatabase { repo })
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
            Some(co) => co,
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
}

impl GitCheckout {
    /// Creates an instance of [`GitCheckout`]. This doesn't imply the checkout
    /// is done. Use [`GitCheckout::is_fresh`] to check.
    ///
    /// * The `repo` will be the checked out Git repository.
    fn new(revision: GitOid, repo: GitRepository) -> Self {
        Self { revision, repo }
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
        let checkout = GitCheckout::new(revision, repo);
        checkout.reset()?;
        Ok(checkout)
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

    /// This performs `git reset --hard` to the revision of this checkout, with
    /// additional interrupt protection by a dummy file [`CHECKOUT_READY_LOCK`].
    ///
    /// If we're interrupted while performing a `git reset` (e.g., we die
    /// because of a signal) Cargo needs to be sure to try to check out this
    /// repo again on the next go-round.
    ///
    /// To enable this we have a dummy file in our checkout, [`.cargo-ok`],
    /// which if present means that the repo has been successfully reset and is
    /// ready to go. Hence if we start to do a reset, we make sure this file
    /// *doesn't* exist, and then once we're done we create the file.
    ///
    /// [`.cargo-ok`]: CHECKOUT_READY_LOCK
    fn reset(&self) -> Result<()> {
        let ok_file = self.repo.path.join(CHECKOUT_READY_LOCK);
        let _ = paths::remove_file(&ok_file);
        debug!("Reset {} to {}", self.repo.path.display(), self.revision);

        // Perform the hard reset.
        ProcessBuilder::new(GIT.as_ref()?)
            .arg("reset")
            .arg("--hard")
            .arg(self.revision.as_str())
            .cwd(&self.repo.path)
            .exec_with_output()?;

        // Update submodules (`git submodule update --recursive`).
        ProcessBuilder::new(GIT.as_ref()?)
            .arg("submodule")
            .arg("update")
            .arg("--recursive")
            .arg("--init")
            .cwd(&self.repo.path)
            .exec_with_output()
            .map(drop)?;

        paths::create(ok_file)?;
        Ok(())
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
    client: &ClientWithMiddleware,
    disable_ssl: bool,
) -> Result<()> {
    let oid_to_fetch = match github_fast_path(repo, remote_url, reference, client) {
        Ok(FastPathRev::UpToDate) => return Ok(()),
        Ok(FastPathRev::NeedsFetch(rev)) => Some(rev),
        Ok(FastPathRev::Indeterminate) => None,
        Err(e) => {
            debug!("Failed to check GitHub {:?}", e);
            None
        }
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
            fetch_with_cli(repo, remote_url, refspecs.as_slice(), tags, disable_ssl)
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
                        disable_ssl,
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
        debug!("Disabling SSL verification for Git fetch");
        cmd.env(EnvVars::GIT_SSL_NO_VERIFY, "true");
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
    cmd.exec_with_output()?;

    Ok(())
}

/// A global cache of the `git lfs` command.
///
/// Returns an error if Git LFS isn't available.
/// Caching the command allows us to only check if LFS is installed once.
static GIT_LFS: LazyLock<Result<ProcessBuilder>> = LazyLock::new(|| {
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
) -> Result<()> {
    let mut cmd = if let Ok(lfs) = GIT_LFS.as_ref() {
        debug!("Fetching Git LFS objects");
        lfs.clone()
    } else {
        // Since this feature is opt-in, warn if not available
        warn!("Git LFS is not available, skipping LFS fetch");
        return Ok(());
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
        .cwd(&repo.path);

    cmd.exec_with_output()?;
    Ok(())
}

/// The result of GitHub fast path check. See [`github_fast_path`] for more.
enum FastPathRev {
    /// The local rev (determined by `reference.resolve(repo)`) is already up to
    /// date with what this rev resolves to on GitHub's server.
    UpToDate,
    /// The following SHA must be fetched in order for the local rev to become
    /// up-to-date.
    NeedsFetch(GitOid),
    /// Don't know whether local rev is up-to-date. We'll fetch _all_ branches
    /// and tags from the server and see what happens.
    Indeterminate,
}

/// Attempts GitHub's special fast path for testing if we've already got an
/// up-to-date copy of the repository.
///
/// Updating the index is done pretty regularly so we want it to be as fast as
/// possible. For registries hosted on GitHub (like the crates.io index) there's
/// a fast path available to use[^1] to tell us that there's no updates to be
/// made.
///
/// Note that this function should never cause an actual failure because it's
/// just a fast path. As a result, a caller should ignore `Err` returned from
/// this function and move forward on the normal path.
///
/// [^1]: <https://developer.github.com/v3/repos/commits/#get-the-sha-1-of-a-commit-reference>
fn github_fast_path(
    git: &mut GitRepository,
    url: &Url,
    reference: ReferenceOrOid<'_>,
    client: &ClientWithMiddleware,
) -> Result<FastPathRev> {
    let Some(GitHubRepository { owner, repo }) = GitHubRepository::parse(url) else {
        return Ok(FastPathRev::Indeterminate);
    };

    let local_object = reference.resolve(git).ok();

    let github_branch_name = match reference {
        ReferenceOrOid::Reference(GitReference::DefaultBranch) => "HEAD",
        ReferenceOrOid::Reference(GitReference::Branch(branch)) => branch,
        ReferenceOrOid::Reference(GitReference::Tag(tag)) => tag,
        ReferenceOrOid::Reference(GitReference::BranchOrTag(branch_or_tag)) => branch_or_tag,
        ReferenceOrOid::Reference(GitReference::NamedRef(rev)) => rev,
        ReferenceOrOid::Reference(GitReference::BranchOrTagOrCommit(rev)) => {
            // `revparse_single` (used by `resolve`) is the only way to turn
            // short hash -> long hash, but it also parses other things,
            // like branch and tag names, which might coincidentally be
            // valid hex.
            //
            // We only return early if `rev` is a prefix of the object found
            // by `revparse_single`. Don't bother talking to GitHub in that
            // case, since commit hashes are permanent. If a commit with the
            // requested hash is already present in the local clone, its
            // contents must be the same as what is on the server for that
            // hash.
            //
            // If `rev` is not found locally by `revparse_single`, we'll
            // need GitHub to resolve it and get a hash. If `rev` is found
            // but is not a short hash of the found object, it's probably a
            // branch and we also need to get a hash from GitHub, in case
            // the branch has moved.
            if let Some(ref local_object) = local_object {
                if is_short_hash_of(rev, *local_object) {
                    return Ok(FastPathRev::UpToDate);
                }
            }
            rev
        }
        ReferenceOrOid::Oid(rev) => {
            debug!("Skipping GitHub fast path; full commit hash provided: {rev}");

            if let Some(local_object) = local_object {
                if rev == local_object {
                    return Ok(FastPathRev::UpToDate);
                }
            }

            // If we know the reference is a full commit hash, we can just return it without
            // querying GitHub.
            return Ok(FastPathRev::NeedsFetch(rev));
        }
    };

    let url = format!("https://api.github.com/repos/{owner}/{repo}/commits/{github_branch_name}");

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    runtime.block_on(async move {
        debug!("Attempting GitHub fast path for: {url}");
        let mut request = client.get(&url);
        request = request.header("Accept", "application/vnd.github.3.sha");
        request = request.header(
            "User-Agent",
            format!("uv/{} (+https://github.com/astral-sh/uv)", version()),
        );
        if let Some(local_object) = local_object {
            request = request.header("If-None-Match", local_object.to_string());
        }

        let response = request.send().await?;

        // GitHub returns a 404 if the repository does not exist, and a 422 if it exists but GitHub
        // is unable to resolve the requested revision.
        response.error_for_status_ref()?;

        let response_code = response.status();
        if response_code == StatusCode::NOT_MODIFIED {
            Ok(FastPathRev::UpToDate)
        } else if response_code == StatusCode::OK {
            let oid_to_fetch = response.text().await?.parse()?;
            Ok(FastPathRev::NeedsFetch(oid_to_fetch))
        } else {
            Ok(FastPathRev::Indeterminate)
        }
    })
}

/// Whether `rev` is a shorter hash of `oid`.
fn is_short_hash_of(rev: &str, oid: GitOid) -> bool {
    let long_hash = oid.to_string();
    match long_hash.get(..rev.len()) {
        Some(truncated_long_hash) => truncated_long_hash.eq_ignore_ascii_case(rev),
        None => false,
    }
}
