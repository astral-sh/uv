//! See [GitSource].

use crate::core::GitReference;
use crate::core::SourceId;
use crate::core::{Dependency, Package, PackageId, Summary};
use crate::sources::git::utils::GitRemote;
use crate::sources::source::MaybePackage;
use crate::sources::source::QueryKind;
use crate::sources::source::Source;
use crate::sources::PathSource;
use crate::util::cache_lock::CacheLockMode;
use crate::util::errors::CargoResult;
use crate::util::hex::short_hash;
use crate::util::Config;
use anyhow::Context;
use cargo_util::paths::exclude_from_backups_and_indexing;
use std::fmt::{self, Debug, Formatter};
use std::task::Poll;
use tracing::trace;
use url::Url;

/// `GitSource` contains one or more packages gathering from a Git repository.
/// Under the hood it uses [`PathSource`] to discover packages inside the
/// repository.
///
/// ## Filesystem layout
///
/// During a successful `GitSource` download, at least two Git repositories are
/// created: one is the shared Git database of this remote, and the other is the
/// Git checkout to a specific revision, which contains the actual files to be
/// compiled. Multiple checkouts can be cloned from a single Git database.
///
/// Those repositories are located at Cargo's Git cache directory
/// `$CARGO_HOME/git`. The file tree of the cache directory roughly looks like:
///
/// ```text
/// $CARGO_HOME/git/
/// ├── checkouts/
/// │  ├── gimli-a0d193bd15a5ed96/
/// │  │  ├── 8e73ef0/     # Git short ID for a certain revision
/// │  │  ├── a2a4b78/
/// │  │  └── e33d1ac/
/// │  ├── log-c58e1db3de7c154d-shallow/
/// │  │  └── 11eda98/
/// └── db/
///    ├── gimli-a0d193bd15a5ed96/
///    └── log-c58e1db3de7c154d-shallow/
/// ```
///
/// For more on Git cache directory, see ["Cargo Home"] in The Cargo Book.
///
/// For more on the directory format `<pkg>-<hash>[-shallow]`, see [`ident`]
/// and [`ident_shallow`].
///
/// ## Locked to a revision
///
/// Once a `GitSource` is fetched, it will resolve to a specific commit revision.
/// This is often mentioned as "locked revision" (`locked_rev`) throughout the
/// codebase. The revision is written into `Cargo.lock`. This is essential since
/// we want to ensure a package can compiles with the same set of files when
/// a `Cargo.lock` is present. With the `locked_rev` provided, `GitSource` can
/// precisely fetch the same revision from the Git repository.
///
/// ["Cargo Home"]: https://doc.rust-lang.org/nightly/cargo/guide/cargo-home.html#directories
pub struct GitSource<'cfg> {
    /// The git remote which we're going to fetch from.
    remote: GitRemote,
    /// The Git reference from the manifest file.
    manifest_reference: GitReference,
    /// The revision which a git source is locked to.
    /// This is expected to be set after the Git repository is fetched.
    locked_rev: Option<git2::Oid>,
    /// The unique identifier of this source.
    source_id: SourceId,
    /// The underlying path source to discover packages inside the Git repository.
    path_source: Option<PathSource<'cfg>>,
    /// The identifier of this source for Cargo's Git cache directory.
    /// See [`ident`] for more.
    ident: String,
    config: &'cfg Config,
    /// Disables status messages.
    quiet: bool,
}

impl<'cfg> GitSource<'cfg> {
    /// Creates a git source for the given [`SourceId`].
    pub fn new(source_id: SourceId, config: &'cfg Config) -> CargoResult<GitSource<'cfg>> {
        assert!(source_id.is_git(), "id is not git, id={}", source_id);

        let remote = GitRemote::new(source_id.url());
        let manifest_reference = source_id.git_reference().unwrap().clone();
        let locked_rev = source_id.precise_git_oid()?;
        let ident = ident_shallow(
            &source_id,
            config
                .cli_unstable()
                .gitoxide
                .map_or(false, |gix| gix.fetch && gix.shallow_deps),
        );

        let source = GitSource {
            remote,
            manifest_reference,
            locked_rev,
            source_id,
            path_source: None,
            ident,
            config,
            quiet: false,
        };

        Ok(source)
    }

    /// Gets the remote repository URL.
    pub fn url(&self) -> &Url {
        self.remote.url()
    }

    /// Returns the packages discovered by this source. It may fetch the Git
    /// repository as well as walk the filesystem if package information
    /// haven't yet updated.
    pub fn read_packages(&mut self) -> CargoResult<Vec<Package>> {
        if self.path_source.is_none() {
            self.invalidate_cache();
            self.block_until_ready()?;
        }
        self.path_source.as_mut().unwrap().read_packages()
    }
}

/// Create an identifier from a URL,
/// essentially turning `proto://host/path/repo` into `repo-<hash-of-url>`.
fn ident(id: &SourceId) -> String {
    let ident = id
        .canonical_url()
        .raw_canonicalized_url()
        .path_segments()
        .and_then(|s| s.rev().next())
        .unwrap_or("");

    let ident = if ident.is_empty() { "_empty" } else { ident };

    format!("{}-{}", ident, short_hash(id.canonical_url()))
}

/// Like [`ident()`], but appends `-shallow` to it, turning
/// `proto://host/path/repo` into `repo-<hash-of-url>-shallow`.
///
/// It's important to separate shallow from non-shallow clones for reasons of
/// backwards compatibility --- older cargo's aren't necessarily handling
/// shallow clones correctly.
fn ident_shallow(id: &SourceId, is_shallow: bool) -> String {
    let mut ident = ident(id);
    if is_shallow {
        ident.push_str("-shallow");
    }
    ident
}

impl<'cfg> Debug for GitSource<'cfg> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "git repo at {}", self.remote.url())?;

        // TODO(-Znext-lockfile-bump): set it to true when stabilizing
        // lockfile v4, because we want Source ID serialization to be
        // consistent with lockfile.
        match self.manifest_reference.pretty_ref(false) {
            Some(s) => write!(f, " ({})", s),
            None => Ok(()),
        }
    }
}

impl<'cfg> Source for GitSource<'cfg> {
    fn query(
        &mut self,
        dep: &Dependency,
        kind: QueryKind,
        f: &mut dyn FnMut(Summary),
    ) -> Poll<CargoResult<()>> {
        if let Some(src) = self.path_source.as_mut() {
            src.query(dep, kind, f)
        } else {
            Poll::Pending
        }
    }

    fn supports_checksums(&self) -> bool {
        false
    }

    fn requires_precise(&self) -> bool {
        true
    }

    fn source_id(&self) -> SourceId {
        self.source_id
    }

    fn block_until_ready(&mut self) -> CargoResult<()> {
        if self.path_source.is_some() {
            return Ok(());
        }

        let git_fs = self.config.git_path();
        // Ignore errors creating it, in case this is a read-only filesystem:
        // perhaps the later operations can succeed anyhow.
        let _ = git_fs.create_dir();
        let git_path = self
            .config
            .assert_package_cache_locked(CacheLockMode::DownloadExclusive, &git_fs);

        // Before getting a checkout, make sure that `<cargo_home>/git` is
        // marked as excluded from indexing and backups. Older versions of Cargo
        // didn't do this, so we do it here regardless of whether `<cargo_home>`
        // exists.
        //
        // This does not use `create_dir_all_excluded_from_backups_atomic` for
        // the same reason: we want to exclude it even if the directory already
        // exists.
        exclude_from_backups_and_indexing(&git_path);

        let db_path = git_path.join("db").join(&self.ident);

        let db = self.remote.db_at(&db_path).ok();
        let (db, actual_rev) = match (self.locked_rev, db) {
            // If we have a locked revision, and we have a preexisting database
            // which has that revision, then no update needs to happen.
            (Some(rev), Some(db)) if db.contains(rev) => (db, rev),

            // If we're in offline mode, we're not locked, and we have a
            // database, then try to resolve our reference with the preexisting
            // repository.
            (None, Some(db)) if self.config.offline() => {
                let rev = db.resolve(&self.manifest_reference).with_context(|| {
                    "failed to lookup reference in preexisting repository, and \
                         can't check for updates in offline mode (--offline)"
                })?;
                (db, rev)
            }

            // ... otherwise we use this state to update the git database. Note
            // that we still check for being offline here, for example in the
            // situation that we have a locked revision but the database
            // doesn't have it.
            (locked_rev, db) => {
                if self.config.offline() {
                    anyhow::bail!(
                        "can't checkout from '{}': you are in the offline mode (--offline)",
                        self.remote.url()
                    );
                }
                if !self.quiet {
                    self.config.shell().status(
                        "Updating",
                        format!("git repository `{}`", self.remote.url()),
                    )?;
                }

                trace!("updating git source `{:?}`", self.remote);

                self.remote.checkout(
                    &db_path,
                    db,
                    &self.manifest_reference,
                    locked_rev,
                    self.config,
                )?
            }
        };

        // Don’t use the full hash, in order to contribute less to reaching the
        // path length limit on Windows. See
        // <https://github.com/servo/servo/pull/14397>.
        let short_id = db.to_short_id(actual_rev)?;

        // Check out `actual_rev` from the database to a scoped location on the
        // filesystem. This will use hard links and such to ideally make the
        // checkout operation here pretty fast.
        let checkout_path = git_path
            .join("checkouts")
            .join(&self.ident)
            .join(short_id.as_str());
        db.copy_to(actual_rev, &checkout_path, self.config)?;

        let source_id = self
            .source_id
            .with_git_precise(Some(actual_rev.to_string()));
        let path_source = PathSource::new_recursive(&checkout_path, source_id, self.config);

        self.path_source = Some(path_source);
        self.locked_rev = Some(actual_rev);
        self.path_source.as_mut().unwrap().update()
    }

    fn download(&mut self, id: PackageId) -> CargoResult<MaybePackage> {
        trace!(
            "getting packages for package ID `{}` from `{:?}`",
            id,
            self.remote
        );
        self.path_source
            .as_mut()
            .expect("BUG: `update()` must be called before `get()`")
            .download(id)
    }

    fn finish_download(&mut self, _id: PackageId, _data: Vec<u8>) -> CargoResult<Package> {
        panic!("no download should have started")
    }

    fn fingerprint(&self, _pkg: &Package) -> CargoResult<String> {
        Ok(self.locked_rev.as_ref().unwrap().to_string())
    }

    fn describe(&self) -> String {
        format!("Git repository {}", self.source_id)
    }

    fn add_to_yanked_whitelist(&mut self, _pkgs: &[PackageId]) {}

    fn is_yanked(&mut self, _pkg: PackageId) -> Poll<CargoResult<bool>> {
        Poll::Ready(Ok(false))
    }

    fn invalidate_cache(&mut self) {}

    fn set_quiet(&mut self, quiet: bool) {
        self.quiet = quiet;
    }
}

#[cfg(test)]
mod test {
    use super::ident;
    use crate::core::{GitReference, SourceId};
    use crate::util::IntoUrl;

    #[test]
    pub fn test_url_to_path_ident_with_path() {
        let ident = ident(&src("https://github.com/carlhuda/cargo"));
        assert!(ident.starts_with("cargo-"));
    }

    #[test]
    pub fn test_url_to_path_ident_without_path() {
        let ident = ident(&src("https://github.com"));
        assert!(ident.starts_with("_empty-"));
    }

    #[test]
    fn test_canonicalize_idents_by_stripping_trailing_url_slash() {
        let ident1 = ident(&src("https://github.com/PistonDevelopers/piston/"));
        let ident2 = ident(&src("https://github.com/PistonDevelopers/piston"));
        assert_eq!(ident1, ident2);
    }

    #[test]
    fn test_canonicalize_idents_by_lowercasing_github_urls() {
        let ident1 = ident(&src("https://github.com/PistonDevelopers/piston"));
        let ident2 = ident(&src("https://github.com/pistondevelopers/piston"));
        assert_eq!(ident1, ident2);
    }

    #[test]
    fn test_canonicalize_idents_by_stripping_dot_git() {
        let ident1 = ident(&src("https://github.com/PistonDevelopers/piston"));
        let ident2 = ident(&src("https://github.com/PistonDevelopers/piston.git"));
        assert_eq!(ident1, ident2);
    }

    #[test]
    fn test_canonicalize_idents_different_protocols() {
        let ident1 = ident(&src("https://github.com/PistonDevelopers/piston"));
        let ident2 = ident(&src("git://github.com/PistonDevelopers/piston"));
        assert_eq!(ident1, ident2);
    }

    fn src(s: &str) -> SourceId {
        SourceId::for_git(&s.into_url().unwrap(), GitReference::DefaultBranch).unwrap()
    }
}
