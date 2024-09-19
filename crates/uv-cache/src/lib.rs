use std::cmp::max;
use std::fmt::{Display, Formatter};
use std::io;
use std::io::Write;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rustc_hash::FxHashSet;
use tracing::debug;

pub use archive::ArchiveId;
use distribution_types::InstalledDist;
use pypi_types::Metadata23;
use uv_cache_info::Timestamp;
use uv_fs::{cachedir, directories};
use uv_normalize::PackageName;

pub use crate::by_timestamp::CachedByTimestamp;
#[cfg(feature = "clap")]
pub use crate::cli::CacheArgs;
pub use crate::removal::{rm_rf, Removal};
pub use crate::wheel::WheelCache;
use crate::wheel::WheelCacheKind;

mod archive;
mod by_timestamp;
#[cfg(feature = "clap")]
mod cli;
mod removal;
mod wheel;

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

    /// Create an ephemeral Python environment in the cache.
    pub fn environment(&self) -> io::Result<tempfile::TempDir> {
        fs_err::create_dir_all(self.bucket(CacheBucket::Builds))?;
        tempfile::tempdir_in(self.bucket(CacheBucket::Builds))
    }

    /// Returns `true` if a cache entry must be revalidated given the [`Refresh`] policy.
    pub fn must_revalidate(&self, package: &PackageName) -> bool {
        match &self.refresh {
            Refresh::None(_) => false,
            Refresh::All(_) => true,
            Refresh::Packages(packages, _) => packages.contains(package),
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
    ) -> io::Result<Freshness> {
        // Grab the cutoff timestamp, if it's relevant.
        let timestamp = match &self.refresh {
            Refresh::None(_) => return Ok(Freshness::Fresh),
            Refresh::All(timestamp) => timestamp,
            Refresh::Packages(packages, timestamp) => {
                if package.map_or(true, |package| packages.contains(package)) {
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
        uv_fs::replace_symlink(archive_entry.path(), path.as_ref())?;

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
    pub fn clear(&self) -> Result<Removal, io::Error> {
        rm_rf(&self.root)
    }

    /// Remove a package from the cache.
    ///
    /// Returns the number of entries removed from the cache.
    pub fn remove(&self, name: &PackageName) -> Result<Removal, io::Error> {
        // Collect the set of referenced archives.
        let before = {
            let mut references = FxHashSet::default();
            for bucket in CacheBucket::iter() {
                let bucket = self.bucket(bucket);
                if bucket.is_dir() {
                    for entry in walkdir::WalkDir::new(bucket) {
                        let entry = entry?;
                        if entry.file_type().is_symlink() {
                            if let Ok(target) = fs_err::canonicalize(entry.path()) {
                                references.insert(target);
                            }
                        }
                    }
                }
            }
            references
        };

        // Remove any entries for the package from the cache.
        let mut summary = Removal::default();
        for bucket in CacheBucket::iter() {
            summary += bucket.remove(self, name)?;
        }

        // Collect the set of referenced archives after the removal.
        let after = {
            let mut references = FxHashSet::default();
            for bucket in CacheBucket::iter() {
                let bucket = self.bucket(bucket);
                if bucket.is_dir() {
                    for entry in walkdir::WalkDir::new(bucket) {
                        let entry = entry?;
                        if entry.file_type().is_symlink() {
                            if let Ok(target) = fs_err::canonicalize(entry.path()) {
                                references.insert(target);
                            }
                        }
                    }
                }
            }
            references
        };

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

                // Remove any symlinks and directories in the revision. The symlinks represent
                // unzipped wheels, and the directories represent the source distribution archives.
                for entry in fs_err::read_dir(entry.path())? {
                    let entry = entry?;
                    let path = entry.path();

                    if path.is_dir() {
                        debug!("Removing unzipped built wheel entry: {}", path.display());
                        summary += rm_rf(path)?;
                    } else if path.is_symlink() {
                        debug!("Removing unzipped built wheel entry: {}", path.display());
                        summary += rm_rf(path)?;
                    }
                }
            }
        }

        // Fourth, remove any unused archives (by searching for archives that are not symlinked).
        let mut references = FxHashSet::default();

        for bucket in CacheBucket::iter() {
            let bucket = self.bucket(bucket);
            if bucket.is_dir() {
                for entry in walkdir::WalkDir::new(bucket) {
                    let entry = entry?;
                    if entry.file_type().is_symlink() {
                        if let Ok(target) = fs_err::canonicalize(entry.path()) {
                            references.insert(target);
                        }
                    }
                }
            }
        }

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
    /// acrhive name.
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
    /// built-wheels-v3/
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
            // Note, next time we change the version we should change the name of this bucket to `source-dists-v0`
            Self::SourceDistributions => "built-wheels-v3",
            Self::FlatIndex => "flat-index-v0",
            Self::Git => "git-v0",
            Self::Interpreter => "interpreter-v2",
            // Note that when bumping this, you'll also need to bump it
            // in crates/uv/tests/cache_clean.rs.
            Self::Simple => "simple-v12",
            Self::Wheels => "wheels-v1",
            Self::Archive => "archive-v0",
            Self::Builds => "builds-v0",
            Self::Environments => "environments-v1",
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
            let Ok(metadata) = rmp_serde::from_slice::<Metadata23>(&metadata) else {
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
                for directory in directories(root) {
                    summary += rm_rf(directory.join(name.to_string()))?;
                }

                // For direct URLs, we expect a directory for every URL, followed by a
                // directory per package (indexed by name).
                let root = cache.bucket(self).join(WheelCacheKind::Url);
                for directory in directories(root) {
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
                for directory in directories(root) {
                    summary += rm_rf(directory.join(name.to_string()))?;
                }

                // For direct URLs, we expect a directory for every URL, followed by a
                // directory per version. To determine whether the URL is relevant, we need to
                // search for a wheel matching the package name.
                let root = cache.bucket(self).join(WheelCacheKind::Url);
                for url in directories(root) {
                    if directories(&url).any(|version| is_match(&version, name)) {
                        summary += rm_rf(url)?;
                    }
                }

                // For local dependencies, we expect a directory for every path, followed by a
                // directory per version. To determine whether the path is relevant, we need to
                // search for a wheel matching the package name.
                let root = cache.bucket(self).join(WheelCacheKind::Path);
                for path in directories(root) {
                    if directories(&path).any(|version| is_match(&version, name)) {
                        summary += rm_rf(path)?;
                    }
                }

                // For Git dependencies, we expect a directory for every repository, followed by a
                // directory for every SHA. To determine whether the SHA is relevant, we need to
                // search for a wheel matching the package name.
                let root = cache.bucket(self).join(WheelCacheKind::Git);
                for repository in directories(root) {
                    for sha in directories(repository) {
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
                for directory in directories(root) {
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

/// A timestamp for an archive, which could be a directory (in which case the modification time is
/// the latest modification time of the `pyproject.toml`, `setup.py`, or `setup.cfg` file in the
/// directory) or a single file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArchiveTimestamp(Timestamp);

impl ArchiveTimestamp {
    /// Return the modification timestamp for an archive, which could be a file (like a wheel or a zip
    /// archive) or a directory containing a Python package.
    ///
    /// If the path is to a directory with no entrypoint (i.e., no `pyproject.toml`, `setup.py`, or
    /// `setup.cfg`), returns `None`.
    pub fn from_path(path: impl AsRef<Path>) -> Result<Option<Self>, io::Error> {
        let metadata = fs_err::metadata(path.as_ref())?;
        if metadata.is_file() {
            Ok(Some(Self(Timestamp::from_metadata(&metadata))))
        } else {
            Self::from_source_tree(path)
        }
    }

    /// Return the modification timestamp for a file.
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, io::Error> {
        let metadata = fs_err::metadata(path.as_ref())?;
        Ok(Self(Timestamp::from_metadata(&metadata)))
    }

    /// Return the modification timestamp for a source tree, i.e., a directory.
    ///
    /// If the source tree doesn't contain an entrypoint (i.e., no `pyproject.toml`, `setup.py`, or
    /// `setup.cfg`), returns `None`.
    pub fn from_source_tree(path: impl AsRef<Path>) -> Result<Option<Self>, io::Error> {
        // Compute the modification timestamp for the `pyproject.toml`, `setup.py`, and
        // `setup.cfg` files, if they exist.
        let pyproject_toml = path
            .as_ref()
            .join("pyproject.toml")
            .metadata()
            .ok()
            .filter(std::fs::Metadata::is_file)
            .as_ref()
            .map(Timestamp::from_metadata);

        let setup_py = path
            .as_ref()
            .join("setup.py")
            .metadata()
            .ok()
            .filter(std::fs::Metadata::is_file)
            .as_ref()
            .map(Timestamp::from_metadata);

        let setup_cfg = path
            .as_ref()
            .join("setup.cfg")
            .metadata()
            .ok()
            .filter(std::fs::Metadata::is_file)
            .as_ref()
            .map(Timestamp::from_metadata);

        // Take the most recent timestamp of the three files.
        let Some(timestamp) = max(pyproject_toml, max(setup_py, setup_cfg)) else {
            return Ok(None);
        };

        Ok(Some(Self(timestamp)))
    }

    /// Return the modification timestamp for an archive.
    pub fn timestamp(&self) -> Timestamp {
        self.0
    }

    /// Returns `true` if the `target` (an installed or cached distribution) is up-to-date with the
    /// source archive (`source`).
    ///
    /// The `target` should be an installed package in a virtual environment, or an unzipped
    /// package in the cache.
    ///
    /// The `source` is a source archive, i.e., a path to a built wheel or a Python package directory.
    pub fn up_to_date_with(source: &Path, target: ArchiveTarget) -> Result<bool, io::Error> {
        let Some(modified_at) = Self::from_path(source)? else {
            // If there's no entrypoint, we can't determine the modification time, so we assume that the
            // target is not up-to-date.
            return Ok(false);
        };
        let created_at = match target {
            ArchiveTarget::Install(installed) => {
                Timestamp::from_path(installed.path().join("METADATA"))?
            }
            ArchiveTarget::Cache(cache) => Timestamp::from_path(cache)?,
        };
        Ok(modified_at.timestamp() <= created_at)
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ArchiveTarget<'a> {
    /// The target is an installed package in a virtual environment.
    Install(&'a InstalledDist),
    /// The target is an unzipped package in the cache.
    Cache(&'a Path),
}

impl PartialOrd for ArchiveTimestamp {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.timestamp().cmp(&other.timestamp()))
    }
}

impl Ord for ArchiveTimestamp {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.timestamp().cmp(&other.timestamp())
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
    Packages(Vec<PackageName>, Timestamp),
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
                    Self::Packages(refresh_package, timestamp)
                }
            }
        }
    }

    /// Return the [`Timestamp`] associated with the refresh policy.
    pub fn timestamp(&self) -> Timestamp {
        match self {
            Self::None(timestamp) => *timestamp,
            Self::Packages(_, timestamp) => *timestamp,
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
            (Self::None(t1), Refresh::Packages(packages, t2)) => {
                Refresh::Packages(packages, max(t1, t2))
            }

            // If the policy is `All`, refresh all packages.
            (Self::All(t1), Refresh::None(t2)) => Refresh::All(max(t1, t2)),
            (Self::All(t1), Refresh::All(t2)) => Refresh::All(max(t1, t2)),
            (Self::All(t1), Refresh::Packages(_packages, t2)) => Refresh::All(max(t1, t2)),

            // If the policy is `Packages`, take the "max" of the two policies.
            (Self::Packages(packages, t1), Refresh::None(t2)) => {
                Refresh::Packages(packages, max(t1, t2))
            }
            (Self::Packages(_packages, t1), Refresh::All(t2)) => Refresh::All(max(t1, t2)),
            (Self::Packages(packages1, t1), Refresh::Packages(packages2, t2)) => Refresh::Packages(
                packages1.into_iter().chain(packages2).collect(),
                max(t1, t2),
            ),
        }
    }
}
