use std::fmt::{Display, Formatter};
use std::io;
use std::io::Write;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;

use rustc_hash::FxHashSet;
use tracing::debug;

pub use archive::ArchiveId;
use uv_cache_info::Timestamp;
use uv_distribution_filename::WheelFilename;
use uv_fs::{cachedir, directories, LockedFile};
use uv_normalize::PackageName;
use uv_pypi_types::ResolutionMetadata;

pub use crate::by_timestamp::CachedByTimestamp;
#[cfg(feature = "clap")]
pub use crate::cli::CacheArgs;
use crate::removal::Remover;
pub use crate::removal::{rm_rf, Removal};
pub use crate::wheel::WheelCache;
use crate::wheel::WheelCacheKind;

mod archive;
mod by_timestamp;
#[cfg(feature = "clap")]
mod cli;
mod removal;
mod wheel;

/// The version of the archive bucket.
///
/// Must be kept in-sync with the version in [`CacheBucket::to_str`].
pub const ARCHIVE_VERSION: u8 = 0;

/// A [`CacheEntry`] which may or may not exist yet.
#[derive(Debug, Clone)]
pub struct CacheEntry(PathBuf);

impl CacheEntry {
    /// Create a new [`CacheEntry`] from a directory and a file name.
    pub fn new(dir: impl Into<PathBuf>, file: impl AsRef<Path>) -> Self {
        Self(dir.into().join(file))
    }

    /// Create a new [`CacheEntry`] from a path.
    pub fn from_path(path: impl Into<PathBuf>) -> Self {
        Self(path.into())
    }

    /// Return the cache entry's parent directory.
    pub fn shard(&self) -> CacheShard {
        CacheShard(self.dir().to_path_buf())
    }

    /// Convert the [`CacheEntry`] into a [`PathBuf`].
    #[inline]
    pub fn into_path_buf(self) -> PathBuf {
        self.0
    }

    /// Return the path to the [`CacheEntry`].
    #[inline]
    pub fn path(&self) -> &Path {
        &self.0
    }

    /// Return the cache entry's parent directory.
    #[inline]
    pub fn dir(&self) -> &Path {
        self.0.parent().expect("Cache entry has no parent")
    }

    /// Create a new [`CacheEntry`] with the given file name.
    #[must_use]
    pub fn with_file(&self, file: impl AsRef<Path>) -> Self {
        Self(self.dir().join(file))
    }

    /// Acquire the [`CacheEntry`] as an exclusive lock.
    pub async fn lock(&self) -> Result<LockedFile, io::Error> {
        fs_err::create_dir_all(self.dir())?;
        LockedFile::acquire(self.path(), self.path().display()).await
    }
}

impl AsRef<Path> for CacheEntry {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

/// A subdirectory within the cache.
#[derive(Debug, Clone)]
pub struct CacheShard(PathBuf);

impl CacheShard {
    /// Return a [`CacheEntry`] within this shard.
    pub fn entry(&self, file: impl AsRef<Path>) -> CacheEntry {
        CacheEntry::new(&self.0, file)
    }

    /// Return a [`CacheShard`] within this shard.
    #[must_use]
    pub fn shard(&self, dir: impl AsRef<Path>) -> Self {
        Self(self.0.join(dir.as_ref()))
    }

    /// Acquire the cache entry as an exclusive lock.
    pub async fn lock(&self) -> Result<LockedFile, io::Error> {
        fs_err::create_dir_all(self.as_ref())?;
        LockedFile::acquire(self.join(".lock"), self.display()).await
    }

    /// Return the [`CacheShard`] as a [`PathBuf`].
    pub fn into_path_buf(self) -> PathBuf {
        self.0
    }
}

impl AsRef<Path> for CacheShard {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

impl Deref for CacheShard {
    type Target = Path;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// The main cache abstraction.
#[derive(Debug, Clone)]
pub struct Cache {
    /// The cache directory.
    root: PathBuf,
    /// The refresh strategy to use when reading from the cache.
    refresh: Refresh,
    /// A temporary cache directory, if the user requested `--no-cache`.
    ///
    /// Included to ensure that the temporary directory exists for the length of the operation, but
    /// is dropped at the end as appropriate.
    temp_dir: Option<Arc<tempfile::TempDir>>,
}

impl Cache {
    /// A persistent cache directory at `root`.
    pub fn from_path(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            refresh: Refresh::None(Timestamp::now()),
            temp_dir: None,
        }
    }

    /// Create a temporary cache directory.
    pub fn temp() -> Result<Self, io::Error> {
        let temp_dir = tempfile::tempdir()?;
        Ok(Self {
            root: temp_dir.path().to_path_buf(),
            refresh: Refresh::None(Timestamp::now()),
            temp_dir: Some(Arc::new(temp_dir)),
        })
    }

    /// Set the [`Refresh`] policy for the cache.
    #[must_use]
    pub fn with_refresh(self, refresh: Refresh) -> Self {
        Self { refresh, ..self }
    }

    /// Return the root of the cache.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Return the [`Refresh`] policy for the cache.
    pub fn refresh(&self) -> &Refresh {
        &self.refresh
    }

    /// The folder for a specific cache bucket
    pub fn bucket(&self, cache_bucket: CacheBucket) -> PathBuf {
        self.root.join(cache_bucket.to_str())
    }

    /// Compute an entry in the cache.
    pub fn shard(&self, cache_bucket: CacheBucket, dir: impl AsRef<Path>) -> CacheShard {
        CacheShard(self.bucket(cache_bucket).join(dir.as_ref()))
    }

    /// Compute an entry in the cache.
    pub fn entry(
        &self,
        cache_bucket: CacheBucket,
        dir: impl AsRef<Path>,
        file: impl AsRef<Path>,
    ) -> CacheEntry {
        CacheEntry::new(self.bucket(cache_bucket).join(dir), file)
    }

    /// Return the path to an archive in the cache.
    pub fn archive(&self, id: &ArchiveId) -> PathBuf {
        self.bucket(CacheBucket::Archive).join(id)
    }

    /// Create a temporary directory to be used as a Python virtual environment.
    pub fn venv_dir(&self) -> io::Result<tempfile::TempDir> {
        fs_err::create_dir_all(self.bucket(CacheBucket::Builds))?;
        tempfile::tempdir_in(self.bucket(CacheBucket::Builds))
    }

    /// Create a temporary directory to be used for executing PEP 517 source distribution builds.
    pub fn build_dir(&self) -> io::Result<tempfile::TempDir> {
        fs_err::create_dir_all(self.bucket(CacheBucket::Builds))?;
        tempfile::tempdir_in(self.bucket(CacheBucket::Builds))
    }

    /// Returns `true` if a cache entry must be revalidated given the [`Refresh`] policy.
    pub fn must_revalidate_package(&self, package: &PackageName) -> bool {
        match &self.refresh {
            Refresh::None(_) => false,
            Refresh::All(_) => true,
            Refresh::Packages(packages, _, _) => packages.contains(package),
        }
    }

    /// Returns `true` if a cache entry must be revalidated given the [`Refresh`] policy.
    pub fn must_revalidate_path(&self, path: &Path) -> bool {
        match &self.refresh {
            Refresh::None(_) => false,
            Refresh::All(_) => true,
            Refresh::Packages(_, paths, _) => paths
                .iter()
                .any(|target| same_file::is_same_file(path, target).unwrap_or(false)),
        }
    }

    /// Returns the [`Freshness`] for a cache entry, validating it against the [`Refresh`] policy.
    ///
    /// A cache entry is considered fresh if it was created after the cache itself was
    /// initialized, or if the [`Refresh`] policy does not require revalidation.
    pub fn freshness(
        &self,
        entry: &CacheEntry,
        package: Option<&PackageName>,
        path: Option<&Path>,
    ) -> io::Result<Freshness> {
        // Grab the cutoff timestamp, if it's relevant.
        let timestamp = match &self.refresh {
            Refresh::None(_) => return Ok(Freshness::Fresh),
            Refresh::All(timestamp) => timestamp,
            Refresh::Packages(packages, paths, timestamp) => {
                if package.is_none_or(|package| packages.contains(package))
                    || path.is_some_and(|path| {
                        paths
                            .iter()
                            .any(|target| same_file::is_same_file(path, target).unwrap_or(false))
                    })
                {
                    timestamp
                } else {
                    return Ok(Freshness::Fresh);
                }
            }
        };

        match fs_err::metadata(entry.path()) {
            Ok(metadata) => {
                if Timestamp::from_metadata(&metadata) >= *timestamp {
                    Ok(Freshness::Fresh)
                } else {
                    Ok(Freshness::Stale)
                }
            }
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(Freshness::Missing),
            Err(err) => Err(err),
        }
    }

    /// Persist a temporary directory to the artifact store, returning its unique ID.
    pub async fn persist(
        &self,
        temp_dir: impl AsRef<Path>,
        path: impl AsRef<Path>,
    ) -> io::Result<ArchiveId> {
        // Create a unique ID for the artifact.
        // TODO(charlie): Support content-addressed persistence via SHAs.
        let id = ArchiveId::new();

        // Move the temporary directory into the directory store.
        let archive_entry = self.entry(CacheBucket::Archive, "", &id);
        fs_err::create_dir_all(archive_entry.dir())?;
        uv_fs::rename_with_retry(temp_dir.as_ref(), archive_entry.path()).await?;

        // Create a symlink to the directory store.
        fs_err::create_dir_all(path.as_ref().parent().expect("Cache entry to have parent"))?;
        self.create_link(&id, path.as_ref())?;

        Ok(id)
    }

    /// Returns `true` if the [`Cache`] is temporary.
    pub fn is_temporary(&self) -> bool {
        self.temp_dir.is_some()
    }

    /// Initialize the [`Cache`].
    pub fn init(self) -> Result<Self, io::Error> {
        let root = &self.root;

        // Create the cache directory, if it doesn't exist.
        fs_err::create_dir_all(root)?;

        // Add the CACHEDIR.TAG.
        cachedir::ensure_tag(root)?;

        // Add the .gitignore.
        match fs_err::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(root.join(".gitignore"))
        {
            Ok(mut file) => file.write_all(b"*")?,
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => (),
            Err(err) => return Err(err),
        }

        // Add an empty .gitignore to the build bucket, to ensure that the cache's own .gitignore
        // doesn't interfere with source distribution builds. Build backends (like hatchling) will
        // traverse upwards to look for .gitignore files.
        fs_err::create_dir_all(root.join(CacheBucket::SourceDistributions.to_str()))?;
        match fs_err::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(
                root.join(CacheBucket::SourceDistributions.to_str())
                    .join(".gitignore"),
            ) {
            Ok(_) => {}
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => (),
            Err(err) => return Err(err),
        }

        // Add a phony .git, if it doesn't exist, to ensure that the cache isn't considered to be
        // part of a Git repository. (Some packages will include Git metadata (like a hash) in the
        // built version if they're in a Git repository, but the cache should be viewed as an
        // isolated store.).
        // We have to put this below the gitignore. Otherwise, if the build backend uses the rust
        // ignore crate it will walk up to the top level .gitignore and ignore its python source
        // files.
        fs_err::OpenOptions::new().create(true).write(true).open(
            root.join(CacheBucket::SourceDistributions.to_str())
                .join(".git"),
        )?;

        Ok(Self {
            root: std::path::absolute(root)?,
            ..self
        })
    }

    /// Clear the cache, removing all entries.
    pub fn clear(&self, reporter: Box<dyn CleanReporter>) -> Result<Removal, io::Error> {
        Remover::new(reporter).rm_rf(&self.root)
    }

    /// Remove a package from the cache.
    ///
    /// Returns the number of entries removed from the cache.
    pub fn remove(&self, name: &PackageName) -> Result<Removal, io::Error> {
        // Collect the set of referenced archives.
        let before = self.find_archive_references()?;

        // Remove any entries for the package from the cache.
        let mut summary = Removal::default();
        for bucket in CacheBucket::iter() {
            summary += bucket.remove(self, name)?;
        }

        // Collect the set of referenced archives after the removal.
        let after = self.find_archive_references()?;

        if before != after {
            // Remove any archives that are no longer referenced.
            for entry in fs_err::read_dir(self.bucket(CacheBucket::Archive))? {
                let entry = entry?;
                let path = fs_err::canonicalize(entry.path())?;
                if !after.contains(&path) && before.contains(&path) {
                    debug!("Removing dangling cache entry: {}", path.display());
                    summary += rm_rf(path)?;
                }
            }
        }

        Ok(summary)
    }

    /// Run the garbage collector on the cache, removing any dangling entries.
    pub fn prune(&self, ci: bool) -> Result<Removal, io::Error> {
        let mut summary = Removal::default();

        // First, remove any top-level directories that are unused. These typically represent
        // outdated cache buckets (e.g., `wheels-v0`, when latest is `wheels-v1`).
        for entry in fs_err::read_dir(&self.root)? {
            let entry = entry?;
            let metadata = entry.metadata()?;

            if entry.file_name() == "CACHEDIR.TAG"
                || entry.file_name() == ".gitignore"
                || entry.file_name() == ".git"
            {
                continue;
            }

            if metadata.is_dir() {
                // If the directory is not a cache bucket, remove it.
                if CacheBucket::iter().all(|bucket| entry.file_name() != bucket.to_str()) {
                    let path = entry.path();
                    debug!("Removing dangling cache bucket: {}", path.display());
                    summary += rm_rf(path)?;
                }
            } else {
                // If the file is not a marker file, remove it.
                let path = entry.path();
                debug!("Removing dangling cache bucket: {}", path.display());
                summary += rm_rf(path)?;
            }
        }

        // Second, remove any cached environments. These are never referenced by symlinks, so we can
        // remove them directly.
        match fs_err::read_dir(self.bucket(CacheBucket::Environments)) {
            Ok(entries) => {
                for entry in entries {
                    let entry = entry?;
                    let path = fs_err::canonicalize(entry.path())?;
                    debug!("Removing dangling cache environment: {}", path.display());
                    summary += rm_rf(path)?;
                }
            }
            Err(err) if err.kind() == io::ErrorKind::NotFound => (),
            Err(err) => return Err(err),
        }

        // Third, if enabled, remove all unzipped wheels, leaving only the wheel archives.
        if ci {
            // Remove the entire pre-built wheel cache, since every entry is an unzipped wheel.
            match fs_err::read_dir(self.bucket(CacheBucket::Wheels)) {
                Ok(entries) => {
                    for entry in entries {
                        let entry = entry?;
                        let path = fs_err::canonicalize(entry.path())?;
                        if path.is_dir() {
                            debug!("Removing unzipped wheel entry: {}", path.display());
                            summary += rm_rf(path)?;
                        }
                    }
                }
                Err(err) if err.kind() == io::ErrorKind::NotFound => (),
                Err(err) => return Err(err),
            }

            for entry in walkdir::WalkDir::new(self.bucket(CacheBucket::SourceDistributions)) {
                let entry = entry?;

                // If the directory contains a `metadata.msgpack`, then it's a built wheel revision.
                if !entry.file_type().is_dir() {
                    continue;
                }

                if !entry.path().join("metadata.msgpack").exists() {
                    continue;
                }

                // Remove everything except the built wheel archive and the metadata.
                for entry in fs_err::read_dir(entry.path())? {
                    let entry = entry?;
                    let path = entry.path();

                    // Retain the resolved metadata (`metadata.msgpack`).
                    if path
                        .file_name()
                        .is_some_and(|file_name| file_name == "metadata.msgpack")
                    {
                        continue;
                    }

                    // Retain any built wheel archives.
                    if path
                        .extension()
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("whl"))
                    {
                        continue;
                    }

                    debug!("Removing unzipped built wheel entry: {}", path.display());
                    summary += rm_rf(path)?;
                }
            }
        }

        // Fourth, remove any unused archives (by searching for archives that are not symlinked).
        let references = self.find_archive_references()?;

        match fs_err::read_dir(self.bucket(CacheBucket::Archive)) {
            Ok(entries) => {
                for entry in entries {
                    let entry = entry?;
                    let path = fs_err::canonicalize(entry.path())?;
                    if !references.contains(&path) {
                        debug!("Removing dangling cache archive: {}", path.display());
                        summary += rm_rf(path)?;
                    }
                }
            }
            Err(err) if err.kind() == io::ErrorKind::NotFound => (),
            Err(err) => return Err(err),
        }

        Ok(summary)
    }

    /// Find all references to entries in the archive bucket.
    ///
    /// Archive entries are often referenced by symlinks in other cache buckets. This method
    /// searches for all such references.
    fn find_archive_references(&self) -> Result<FxHashSet<PathBuf>, io::Error> {
        let mut references = FxHashSet::default();
        for bucket in CacheBucket::iter() {
            let bucket_path = self.bucket(bucket);
            if bucket_path.is_dir() {
                for entry in walkdir::WalkDir::new(bucket_path) {
                    let entry = entry?;

                    // Ignore any `.lock` files.
                    if entry
                        .path()
                        .extension()
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("lock"))
                    {
                        continue;
                    }

                    let Some(filename) = entry
                        .path()
                        .file_name()
                        .and_then(|file_name| file_name.to_str())
                    else {
                        continue;
                    };

                    if bucket == CacheBucket::Wheels {
                        // In the `wheels` bucket, we often use a hash of the filename as the
                        // directory name, so we can't rely on the stem.
                        //
                        // Instead, we skip if it contains an extension (e.g., `.whl`, `.http`,
                        // `.rev`, and `.msgpack` files).
                        if filename
                            .rsplit_once('-') // strip version/tags, might contain a dot ('.')
                            .is_none_or(|(_, suffix)| suffix.contains('.'))
                        {
                            continue;
                        }
                    } else {
                        // For other buckets only include entries that match the wheel stem pattern (e.g., `typing-extensions-4.8.0-py3-none-any`).
                        if WheelFilename::from_stem(filename).is_err() {
                            continue;
                        }
                    }

                    if let Ok(target) = self.resolve_link(entry.path()) {
                        references.insert(target);
                    }
                }
            }
        }
        Ok(references)
    }

    /// Create a link to a directory in the archive bucket.
    ///
    /// On Windows, we write structured data ([`Link`]) to a file containing the archive ID and
    /// version. On Unix, we create a symlink to the target directory.
    #[cfg(windows)]
    pub fn create_link(&self, id: &ArchiveId, dst: impl AsRef<Path>) -> io::Result<()> {
        // Serialize the link.
        let link = Link::new(id.clone());
        let contents = link.to_string();

        // First, attempt to create a file at the location, but fail if it already exists.
        match fs_err::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(dst.as_ref())
        {
            Ok(mut file) => {
                // Write the target path to the file.
                file.write_all(contents.as_bytes())?;
                Ok(())
            }
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {
                // Write to a temporary file, then move it into place.
                let temp_dir = tempfile::tempdir_in(dst.as_ref().parent().unwrap())?;
                let temp_file = temp_dir.path().join("link");
                fs_err::write(&temp_file, contents.as_bytes())?;

                // Move the symlink into the target location.
                fs_err::rename(&temp_file, dst.as_ref())?;

                Ok(())
            }
            Err(err) => Err(err),
        }
    }

    /// Resolve an archive link, returning the fully-resolved path.
    ///
    /// Returns an error if the link target does not exist.
    #[cfg(windows)]
    pub fn resolve_link(&self, path: impl AsRef<Path>) -> io::Result<PathBuf> {
        // Deserialize the link.
        let contents = fs_err::read_to_string(path.as_ref())?;
        let link = Link::from_str(&contents)?;

        // Ignore stale links.
        if link.version != ARCHIVE_VERSION {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "The link target does not exist.",
            ));
        }

        // Reconstruct the path.
        let path = self.archive(&link.id);
        path.canonicalize()
    }

    /// Create a link to a directory in the archive bucket.
    ///
    /// On Windows, we write structured data ([`Link`]) to a file containing the archive ID and
    /// version. On Unix, we create a symlink to the target directory.
    #[cfg(unix)]
    pub fn create_link(&self, id: &ArchiveId, dst: impl AsRef<Path>) -> io::Result<()> {
        // Construct the link target.
        let src = self.archive(id);
        let dst = dst.as_ref();

        // Attempt to create the symlink directly.
        match std::os::unix::fs::symlink(&src, dst) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {
                // Create a symlink, using a temporary file to ensure atomicity.
                let temp_dir = tempfile::tempdir_in(dst.parent().unwrap())?;
                let temp_file = temp_dir.path().join("link");
                std::os::unix::fs::symlink(&src, &temp_file)?;

                // Move the symlink into the target location.
                fs_err::rename(&temp_file, dst)?;

                Ok(())
            }
            Err(err) => Err(err),
        }
    }

    /// Resolve an archive link, returning the fully-resolved path.
    ///
    /// Returns an error if the link target does not exist.
    #[cfg(unix)]
    pub fn resolve_link(&self, path: impl AsRef<Path>) -> io::Result<PathBuf> {
        path.as_ref().canonicalize()
    }
}

/// An archive (unzipped wheel) that exists in the local cache.
#[derive(Debug, Clone)]
#[allow(unused)]
struct Link {
    /// The unique ID of the entry in the archive bucket.
    id: ArchiveId,
    /// The version of the archive bucket.
    version: u8,
}

#[allow(unused)]
impl Link {
    /// Create a new [`Archive`] with the given ID and hashes.
    fn new(id: ArchiveId) -> Self {
        Self {
            id,
            version: ARCHIVE_VERSION,
        }
    }
}

impl Display for Link {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "archive-v{}/{}", self.version, self.id)
    }
}

impl FromStr for Link {
    type Err = io::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut parts = s.splitn(2, '/');
        let version = parts
            .next()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing version"))?;
        let id = parts
            .next()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing ID"))?;

        // Parse the archive version from `archive-v{version}/{id}`.
        let version = version
            .strip_prefix("archive-v")
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing version prefix"))?;
        let version = u8::from_str(version).map_err(|err| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("failed to parse version: {err}"),
            )
        })?;

        // Parse the ID from `archive-v{version}/{id}`.
        let id = ArchiveId::from_str(id).map_err(|err| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("failed to parse ID: {err}"),
            )
        })?;

        Ok(Self { id, version })
    }
}

pub trait CleanReporter: Send + Sync {
    /// Called after one file or directory is removed.
    fn on_clean(&self);

    /// Called after all files and directories are removed.
    fn on_complete(&self);
}

/// The different kinds of data in the cache are stored in different bucket, which in our case
/// are subdirectories of the cache root.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum CacheBucket {
    /// Wheels (excluding built wheels), alongside their metadata and cache policy.
    ///
    /// There are three kinds from cache entries: Wheel metadata and policy as `MsgPack` files, the
    /// wheels themselves, and the unzipped wheel archives. If a wheel file is over an in-memory
    /// size threshold, we first download the zip file into the cache, then unzip it into a
    /// directory with the same name (exclusive of the `.whl` extension).
    ///
    /// Cache structure:
    ///  * `wheel-metadata-v0/pypi/foo/{foo-1.0.0-py3-none-any.msgpack, foo-1.0.0-py3-none-any.whl}`
    ///  * `wheel-metadata-v0/<digest(index-url)>/foo/{foo-1.0.0-py3-none-any.msgpack, foo-1.0.0-py3-none-any.whl}`
    ///  * `wheel-metadata-v0/url/<digest(url)>/foo/{foo-1.0.0-py3-none-any.msgpack, foo-1.0.0-py3-none-any.whl}`
    ///
    /// See `uv_client::RegistryClient::wheel_metadata` for information on how wheel metadata
    /// is fetched.
    ///
    /// # Example
    ///
    /// Consider the following `requirements.in`:
    /// ```text
    /// # pypi wheel
    /// pandas
    /// # url wheel
    /// flask @ https://files.pythonhosted.org/packages/36/42/015c23096649b908c809c69388a805a571a3bea44362fe87e33fc3afa01f/flask-3.0.0-py3-none-any.whl
    /// ```
    ///
    /// When we run `pip compile`, it will only fetch and cache the metadata (and cache policy), it
    /// doesn't need the actual wheels yet:
    /// ```text
    /// wheel-v0
    /// ├── pypi
    /// │   ...
    /// │   ├── pandas
    /// │   │   └── pandas-2.1.3-cp310-cp310-manylinux_2_17_x86_64.manylinux2014_x86_64.msgpack
    /// │   ...
    /// └── url
    ///     └── 4b8be67c801a7ecb
    ///         └── flask
    ///             └── flask-3.0.0-py3-none-any.msgpack
    /// ```
    ///
    /// We get the following `requirement.txt` from `pip compile`:
    ///
    /// ```text
    /// [...]
    /// flask @ https://files.pythonhosted.org/packages/36/42/015c23096649b908c809c69388a805a571a3bea44362fe87e33fc3afa01f/flask-3.0.0-py3-none-any.whl
    /// [...]
    /// pandas==2.1.3
    /// [...]
    /// ```
    ///
    /// If we run `pip sync` on `requirements.txt` on a different machine, it also fetches the
    /// wheels:
    ///
    /// TODO(konstin): This is still wrong, we need to store the cache policy too!
    /// ```text
    /// wheel-v0
    /// ├── pypi
    /// │   ...
    /// │   ├── pandas
    /// │   │   ├── pandas-2.1.3-cp310-cp310-manylinux_2_17_x86_64.manylinux2014_x86_64.whl
    /// │   │   ├── pandas-2.1.3-cp310-cp310-manylinux_2_17_x86_64.manylinux2014_x86_64
    /// │   ...
    /// └── url
    ///     └── 4b8be67c801a7ecb
    ///         └── flask
    ///             └── flask-3.0.0-py3-none-any.whl
    ///                 ├── flask
    ///                 │   └── ...
    ///                 └── flask-3.0.0.dist-info
    ///                     └── ...
    /// ```
    ///
    /// If we run first `pip compile` and then `pip sync` on the same machine, we get both:
    ///
    /// ```text
    /// wheels-v0
    /// ├── pypi
    /// │   ├── ...
    /// │   ├── pandas
    /// │   │   ├── pandas-2.1.3-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.msgpack
    /// │   │   ├── pandas-2.1.3-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl
    /// │   │   └── pandas-2.1.3-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64
    /// │   │       ├── pandas
    /// │   │       │   ├── ...
    /// │   │       ├── pandas-2.1.3.dist-info
    /// │   │       │   ├── ...
    /// │   │       └── pandas.libs
    /// │   ├── ...
    /// └── url
    ///     └── 4b8be67c801a7ecb
    ///         └── flask
    ///             ├── flask-3.0.0-py3-none-any.msgpack
    ///             ├── flask-3.0.0-py3-none-any.msgpack
    ///             └── flask-3.0.0-py3-none-any
    ///                 ├── flask
    ///                 │   └── ...
    ///                 └── flask-3.0.0.dist-info
    ///                     └── ...
    Wheels,
    /// Source distributions, wheels built from source distributions, their extracted metadata, and the
    /// cache policy of the source distribution.
    ///
    /// The structure is similar of that of the `Wheel` bucket, except we have an additional layer
    /// for the source distribution filename and the metadata is at the source distribution-level,
    /// not at the wheel level.
    ///
    /// TODO(konstin): The cache policy should be on the source distribution level, the metadata we
    /// can put next to the wheels as in the `Wheels` bucket.
    ///
    /// The unzipped source distribution is stored in a directory matching the source distribution
    /// archive name.
    ///
    /// Source distributions are built into zipped wheel files (as PEP 517 specifies) and unzipped
    /// lazily before installing. So when resolving, we only build the wheel and store the archive
    /// file in the cache, when installing, we unpack it under the same name (exclusive of the
    /// `.whl` extension). You may find a mix of wheel archive zip files and unzipped wheel
    /// directories in the cache.
    ///
    /// Cache structure:
    ///  * `built-wheels-v0/pypi/foo/34a17436ed1e9669/{manifest.msgpack, metadata.msgpack, foo-1.0.0.zip, foo-1.0.0-py3-none-any.whl, ...other wheels}`
    ///  * `built-wheels-v0/<digest(index-url)>/foo/foo-1.0.0.zip/{manifest.msgpack, metadata.msgpack, foo-1.0.0-py3-none-any.whl, ...other wheels}`
    ///  * `built-wheels-v0/url/<digest(url)>/foo/foo-1.0.0.zip/{manifest.msgpack, metadata.msgpack, foo-1.0.0-py3-none-any.whl, ...other wheels}`
    ///  * `built-wheels-v0/git/<digest(url)>/<git sha>/foo/foo-1.0.0.zip/{metadata.msgpack, foo-1.0.0-py3-none-any.whl, ...other wheels}`
    ///
    /// But the url filename does not need to be a valid source dist filename
    /// (<https://github.com/search?q=path%3A**%2Frequirements.txt+master.zip&type=code>),
    /// so it could also be the following and we have to take any string as filename:
    ///  * `built-wheels-v0/url/<sha256(url)>/master.zip/metadata.msgpack`
    ///
    /// # Example
    ///
    /// The following requirements:
    /// ```text
    /// # git source dist
    /// pydantic-extra-types @ git+https://github.com/pydantic/pydantic-extra-types.git
    /// # pypi source dist
    /// django_allauth==0.51.0
    /// # url source dist
    /// werkzeug @ https://files.pythonhosted.org/packages/0d/cc/ff1904eb5eb4b455e442834dabf9427331ac0fa02853bf83db817a7dd53d/werkzeug-3.0.1.tar.gz
    /// ```
    ///
    /// ...may be cached as:
    /// ```text
    /// built-wheels-v4/
    /// ├── git
    /// │   └── 2122faf3e081fb7a
    /// │       └── 7a2d650a4a7b4d04
    /// │           ├── metadata.msgpack
    /// │           └── pydantic_extra_types-2.9.0-py3-none-any.whl
    /// ├── pypi
    /// │   └── django-allauth
    /// │       └── 0.51.0
    /// │           ├── 0gH-_fwv8tdJ7JwwjJsUc
    /// │           │   ├── django-allauth-0.51.0.tar.gz
    /// │           │   │   └── [UNZIPPED CONTENTS]
    /// │           │   ├── django_allauth-0.51.0-py3-none-any.whl
    /// │           │   └── metadata.msgpack
    /// │           └── revision.http
    /// └── url
    ///     └── 6781bd6440ae72c2
    ///         ├── APYY01rbIfpAo_ij9sCY6
    ///         │   ├── metadata.msgpack
    ///         │   ├── werkzeug-3.0.1-py3-none-any.whl
    ///         │   └── werkzeug-3.0.1.tar.gz
    ///         │       └── [UNZIPPED CONTENTS]
    ///         └── revision.http
    /// ```
    ///
    /// Structurally, the `manifest.msgpack` is empty, and only contains the caching information
    /// needed to invalidate the cache. The `metadata.msgpack` contains the metadata of the source
    /// distribution.
    SourceDistributions,
    /// Flat index responses, a format very similar to the simple metadata API.
    ///
    /// Cache structure:
    ///  * `flat-index-v0/index/<digest(flat_index_url)>.msgpack`
    ///
    /// The response is stored as `Vec<File>`.
    FlatIndex,
    /// Git repositories.
    Git,
    /// Information about an interpreter at a path.
    ///
    /// To avoid caching pyenv shims, bash scripts which may redirect to a new python version
    /// without the shim itself changing, we only cache when the path equals `sys.executable`, i.e.
    /// the path we're running is the python executable itself and not a shim.
    ///
    /// Cache structure: `interpreter-v0/<digest(path)>.msgpack`
    ///
    /// # Example
    ///
    /// The contents of each of the `MsgPack` files has a timestamp field in unix time, the [PEP 508]
    /// markers and some information from the `sys`/`sysconfig` modules.
    ///
    /// ```json
    /// {
    ///   "timestamp": 1698047994491,
    ///   "data": {
    ///     "markers": {
    ///       "implementation_name": "cpython",
    ///       "implementation_version": "3.12.0",
    ///       "os_name": "posix",
    ///       "platform_machine": "x86_64",
    ///       "platform_python_implementation": "CPython",
    ///       "platform_release": "6.5.0-13-generic",
    ///       "platform_system": "Linux",
    ///       "platform_version": "#13-Ubuntu SMP PREEMPT_DYNAMIC Fri Nov  3 12:16:05 UTC 2023",
    ///       "python_full_version": "3.12.0",
    ///       "python_version": "3.12",
    ///       "sys_platform": "linux"
    ///     },
    ///     "base_exec_prefix": "/home/ferris/.pyenv/versions/3.12.0",
    ///     "base_prefix": "/home/ferris/.pyenv/versions/3.12.0",
    ///     "sys_executable": "/home/ferris/projects/uv/.venv/bin/python"
    ///   }
    /// }
    /// ```
    ///
    /// [PEP 508]: https://peps.python.org/pep-0508/#environment-markers
    Interpreter,
    /// Index responses through the simple metadata API.
    ///
    /// Cache structure:
    ///  * `simple-v0/pypi/<package_name>.rkyv`
    ///  * `simple-v0/<digest(index_url)>/<package_name>.rkyv`
    ///
    /// The response is parsed into `uv_client::SimpleMetadata` before storage.
    Simple,
    /// A cache of unzipped wheels, stored as directories. This is used internally within the cache.
    /// When other buckets need to store directories, they should persist them to
    /// [`CacheBucket::Archive`], and then symlink them into the appropriate bucket. This ensures
    /// that cache entries can be atomically replaced and removed, as storing directories in the
    /// other buckets directly would make atomic operations impossible.
    Archive,
    /// Ephemeral virtual environments used to execute PEP 517 builds and other operations.
    Builds,
    /// Reusable virtual environments used to invoke Python tools.
    Environments,
}

impl CacheBucket {
    fn to_str(self) -> &'static str {
        match self {
            // Note that when bumping this, you'll also need to bump it
            // in `crates/uv/tests/it/cache_prune.rs`.
            Self::SourceDistributions => "sdists-v8",
            Self::FlatIndex => "flat-index-v2",
            Self::Git => "git-v0",
            Self::Interpreter => "interpreter-v4",
            // Note that when bumping this, you'll also need to bump it
            // in `crates/uv/tests/it/cache_clean.rs`.
            Self::Simple => "simple-v15",
            // Note that when bumping this, you'll also need to bump it
            // in `crates/uv/tests/it/cache_prune.rs`.
            Self::Wheels => "wheels-v5",
            // Note that when bumping this, you'll also need to bump
            // `ARCHIVE_VERSION` in `crates/uv-cache/src/lib.rs`.
            Self::Archive => "archive-v0",
            Self::Builds => "builds-v0",
            Self::Environments => "environments-v2",
        }
    }

    /// Remove a package from the cache bucket.
    ///
    /// Returns the number of entries removed from the cache.
    fn remove(self, cache: &Cache, name: &PackageName) -> Result<Removal, io::Error> {
        /// Returns `true` if the [`Path`] represents a built wheel for the given package.
        fn is_match(path: &Path, name: &PackageName) -> bool {
            let Ok(metadata) = fs_err::read(path.join("metadata.msgpack")) else {
                return false;
            };
            let Ok(metadata) = rmp_serde::from_slice::<ResolutionMetadata>(&metadata) else {
                return false;
            };
            metadata.name == *name
        }

        let mut summary = Removal::default();
        match self {
            Self::Wheels => {
                // For `pypi` wheels, we expect a directory per package (indexed by name).
                let root = cache.bucket(self).join(WheelCacheKind::Pypi);
                summary += rm_rf(root.join(name.to_string()))?;

                // For alternate indices, we expect a directory for every index (under an `index`
                // subdirectory), followed by a directory per package (indexed by name).
                let root = cache.bucket(self).join(WheelCacheKind::Index);
                for directory in directories(root)? {
                    summary += rm_rf(directory.join(name.to_string()))?;
                }

                // For direct URLs, we expect a directory for every URL, followed by a
                // directory per package (indexed by name).
                let root = cache.bucket(self).join(WheelCacheKind::Url);
                for directory in directories(root)? {
                    summary += rm_rf(directory.join(name.to_string()))?;
                }
            }
            Self::SourceDistributions => {
                // For `pypi` wheels, we expect a directory per package (indexed by name).
                let root = cache.bucket(self).join(WheelCacheKind::Pypi);
                summary += rm_rf(root.join(name.to_string()))?;

                // For alternate indices, we expect a directory for every index (under an `index`
                // subdirectory), followed by a directory per package (indexed by name).
                let root = cache.bucket(self).join(WheelCacheKind::Index);
                for directory in directories(root)? {
                    summary += rm_rf(directory.join(name.to_string()))?;
                }

                // For direct URLs, we expect a directory for every URL, followed by a
                // directory per version. To determine whether the URL is relevant, we need to
                // search for a wheel matching the package name.
                let root = cache.bucket(self).join(WheelCacheKind::Url);
                for url in directories(root)? {
                    if directories(&url)?.any(|version| is_match(&version, name)) {
                        summary += rm_rf(url)?;
                    }
                }

                // For local dependencies, we expect a directory for every path, followed by a
                // directory per version. To determine whether the path is relevant, we need to
                // search for a wheel matching the package name.
                let root = cache.bucket(self).join(WheelCacheKind::Path);
                for path in directories(root)? {
                    if directories(&path)?.any(|version| is_match(&version, name)) {
                        summary += rm_rf(path)?;
                    }
                }

                // For Git dependencies, we expect a directory for every repository, followed by a
                // directory for every SHA. To determine whether the SHA is relevant, we need to
                // search for a wheel matching the package name.
                let root = cache.bucket(self).join(WheelCacheKind::Git);
                for repository in directories(root)? {
                    for sha in directories(repository)? {
                        if is_match(&sha, name) {
                            summary += rm_rf(sha)?;
                        }
                    }
                }
            }
            Self::Simple => {
                // For `pypi` wheels, we expect a rkyv file per package, indexed by name.
                let root = cache.bucket(self).join(WheelCacheKind::Pypi);
                summary += rm_rf(root.join(format!("{name}.rkyv")))?;

                // For alternate indices, we expect a directory for every index (under an `index`
                // subdirectory), followed by a directory per package (indexed by name).
                let root = cache.bucket(self).join(WheelCacheKind::Index);
                for directory in directories(root)? {
                    summary += rm_rf(directory.join(format!("{name}.rkyv")))?;
                }
            }
            Self::FlatIndex => {
                // We can't know if the flat index includes a package, so we just remove the entire
                // cache entry.
                let root = cache.bucket(self);
                summary += rm_rf(root)?;
            }
            Self::Git => {
                // Nothing to do.
            }
            Self::Interpreter => {
                // Nothing to do.
            }
            Self::Archive => {
                // Nothing to do.
            }
            Self::Builds => {
                // Nothing to do.
            }
            Self::Environments => {
                // Nothing to do.
            }
        }
        Ok(summary)
    }

    /// Return an iterator over all cache buckets.
    pub fn iter() -> impl Iterator<Item = Self> {
        [
            Self::Wheels,
            Self::SourceDistributions,
            Self::FlatIndex,
            Self::Git,
            Self::Interpreter,
            Self::Simple,
            Self::Archive,
            Self::Builds,
            Self::Environments,
        ]
        .iter()
        .copied()
    }
}

impl Display for CacheBucket {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.to_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Freshness {
    /// The cache entry is fresh according to the [`Refresh`] policy.
    Fresh,
    /// The cache entry is stale according to the [`Refresh`] policy.
    Stale,
    /// The cache entry does not exist.
    Missing,
}

impl Freshness {
    pub const fn is_fresh(self) -> bool {
        matches!(self, Self::Fresh)
    }

    pub const fn is_stale(self) -> bool {
        matches!(self, Self::Stale)
    }
}

/// A refresh policy for cache entries.
#[derive(Debug, Clone)]
pub enum Refresh {
    /// Don't refresh any entries.
    None(Timestamp),
    /// Refresh entries linked to the given packages, if created before the given timestamp.
    Packages(Vec<PackageName>, Vec<PathBuf>, Timestamp),
    /// Refresh all entries created before the given timestamp.
    All(Timestamp),
}

impl Refresh {
    /// Determine the refresh strategy to use based on the command-line arguments.
    pub fn from_args(refresh: Option<bool>, refresh_package: Vec<PackageName>) -> Self {
        let timestamp = Timestamp::now();
        match refresh {
            Some(true) => Self::All(timestamp),
            Some(false) => Self::None(timestamp),
            None => {
                if refresh_package.is_empty() {
                    Self::None(timestamp)
                } else {
                    Self::Packages(refresh_package, vec![], timestamp)
                }
            }
        }
    }

    /// Return the [`Timestamp`] associated with the refresh policy.
    pub fn timestamp(&self) -> Timestamp {
        match self {
            Self::None(timestamp) => *timestamp,
            Self::Packages(.., timestamp) => *timestamp,
            Self::All(timestamp) => *timestamp,
        }
    }

    /// Returns `true` if no packages should be reinstalled.
    pub fn is_none(&self) -> bool {
        matches!(self, Self::None(_))
    }

    /// Combine two [`Refresh`] policies, taking the "max" of the two policies.
    #[must_use]
    pub fn combine(self, other: Refresh) -> Self {
        /// Return the maximum of two timestamps.
        fn max(a: Timestamp, b: Timestamp) -> Timestamp {
            if a > b {
                a
            } else {
                b
            }
        }

        match (self, other) {
            // If the policy is `None`, return the existing refresh policy.
            // Take the `max` of the two timestamps.
            (Self::None(t1), Refresh::None(t2)) => Refresh::None(max(t1, t2)),
            (Self::None(t1), Refresh::All(t2)) => Refresh::All(max(t1, t2)),
            (Self::None(t1), Refresh::Packages(packages, paths, t2)) => {
                Refresh::Packages(packages, paths, max(t1, t2))
            }

            // If the policy is `All`, refresh all packages.
            (Self::All(t1), Refresh::None(t2)) => Refresh::All(max(t1, t2)),
            (Self::All(t1), Refresh::All(t2)) => Refresh::All(max(t1, t2)),
            (Self::All(t1), Refresh::Packages(.., t2)) => Refresh::All(max(t1, t2)),

            // If the policy is `Packages`, take the "max" of the two policies.
            (Self::Packages(packages, paths, t1), Refresh::None(t2)) => {
                Refresh::Packages(packages, paths, max(t1, t2))
            }
            (Self::Packages(.., t1), Refresh::All(t2)) => Refresh::All(max(t1, t2)),
            (Self::Packages(packages1, paths1, t1), Refresh::Packages(packages2, paths2, t2)) => {
                Refresh::Packages(
                    packages1.into_iter().chain(packages2).collect(),
                    paths1.into_iter().chain(paths2).collect(),
                    max(t1, t2),
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use crate::ArchiveId;

    use super::Link;

    #[test]
    fn test_link_round_trip() {
        let id = ArchiveId::new();
        let link = Link::new(id);
        let s = link.to_string();
        let parsed = Link::from_str(&s).unwrap();
        assert_eq!(link.id, parsed.id);
        assert_eq!(link.version, parsed.version);
    }

    #[test]
    fn test_link_deserialize() {
        assert!(Link::from_str("archive-v0/foo").is_ok());
        assert!(Link::from_str("archive/foo").is_err());
        assert!(Link::from_str("v1/foo").is_err());
        assert!(Link::from_str("archive-v0/").is_err());
    }
}
