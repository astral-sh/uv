use std::fmt::{Display, Formatter};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tempfile::{tempdir, TempDir};

pub use canonical_url::{CanonicalUrl, RepositoryUrl};
#[cfg(feature = "clap")]
pub use cli::CacheArgs;
pub use digest::digest;
pub use stable_hash::{StableHash, StableHasher};
pub use wheel_and_metadata::WheelAndMetadataCache;

mod cache_key;
mod canonical_url;
mod cli;
mod digest;
mod stable_hash;
mod wheel_and_metadata;

/// A cache entry which may or may not exist yet.
#[derive(Debug, Clone)]
pub struct CacheEntry {
    pub dir: PathBuf,
    pub file: String,
}

impl CacheEntry {
    pub fn path(&self) -> PathBuf {
        // TODO(konstin): Cache this to avoid allocations?
        self.dir.join(&self.file)
    }
}

/// The main cache abstraction.
#[derive(Debug, Clone)]
pub struct Cache {
    /// The cache directory.
    root: PathBuf,
    /// A temporary cache directory, if the user requested `--no-cache`.
    ///
    /// Included to ensure that the temporary directory exists for the length of the operation, but
    /// is dropped at the end as appropriate.
    _temp_dir_drop: Option<Arc<TempDir>>,
}

impl Cache {
    /// A persistent cache directory at `root`.
    pub fn from_path(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            _temp_dir_drop: None,
        }
    }

    /// Create a temporary cache directory.
    pub fn temp() -> Result<Self, io::Error> {
        let temp_dir = tempdir()?;
        Ok(Self {
            root: temp_dir.path().to_path_buf(),
            _temp_dir_drop: Some(Arc::new(temp_dir)),
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The folder for a specific cache bucket
    pub fn bucket(&self, cache_bucket: CacheBucket) -> PathBuf {
        self.root.join(cache_bucket.to_str())
    }

    pub fn entry(
        &self,
        cache_bucket: CacheBucket,
        dir: impl AsRef<Path>,
        file: String,
    ) -> CacheEntry {
        CacheEntry {
            dir: self.bucket(cache_bucket).join(dir.as_ref()),
            file,
        }
    }
}

/// The different kinds of data in the cache are stored in different bucket, which in our case
/// are subfolders.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum CacheBucket {
    /// Wheels (excluding built wheels), their metadata and cache policy.
    ///
    /// There are two kinds from cache entries: Wheel metadata and policy as json files, and the
    /// wheels themselves. If a wheel file is over an in-memory size threshold, we first download
    /// the zip file into the cache, then unzip it into a directory with the same name. Unlike the
    /// built wheels cache, you should only see unpacked wheel directories after running puffin.
    ///
    /// Cache structure:
    ///  * `wheel-metadata-v0/pypi/{foo-1.0.0-py3-none-any.json, foo-1.0.0-py3-none-any.whl}`
    ///  * `wheel-metadata-v0/<digest(index-url)>/{foo-1.0.0-py3-none-any.json, foo-1.0.0-py3-none-any.whl}`
    ///  * `wheel-metadata-v0/url/<digest(url)>/{foo-1.0.0-py3-none-any.json, foo-1.0.0-py3-none-any.whl}`
    ///
    /// See `puffin_client::RegistryClient::wheel_metadata` for information on how wheel metadata
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
    /// When we run `pip-compile`, it will only fetch and cache the metadata (and cache policy), it
    /// doesn't need the actual wheels yet:
    /// ```text
    /// wheel-v0
    /// ├── pypi
    /// │   ...
    /// │   ├── pandas-2.1.3-cp310-cp310-manylinux_2_17_x86_64.manylinux2014_x86_64.json
    /// │   ...
    /// └── url
    ///     └── 4b8be67c801a7ecb
    ///         └── flask-3.0.0-py3-none-any.json
    /// ```
    ///
    /// We get the following `requirement.txt` from `pip-compile`:
    ///
    /// ```text
    /// [...]
    /// flask @ https://files.pythonhosted.org/packages/36/42/015c23096649b908c809c69388a805a571a3bea44362fe87e33fc3afa01f/flask-3.0.0-py3-none-any.whl
    /// [...]
    /// pandas==2.1.3
    /// [...]
    /// ```
    ///
    /// If we run `pip-sync` on `requirements.txt` on a different machine, it also fetches the
    /// wheels:
    ///
    /// TODO(konstin): This is still wrong, we need to store the cache policy too!
    /// ```text
    /// wheel-v0
    /// ├── pypi
    /// │   ...
    /// │   ├── pandas-2.1.3-cp310-cp310-manylinux_2_17_x86_64.manylinux2014_x86_64.whl
    /// │   ...
    /// └── url
    ///     └── 4b8be67c801a7ecb
    ///         └── flask-3.0.0-py3-none-any.whl
    ///             ├── flask
    ///             │   └── ...
    ///             └── flask-3.0.0.dist-info
    ///                 └── ...
    /// ```
    ///
    /// If we run first `pip-compile` and then `pip-sync` on the same machine, we get both:
    ///
    /// ```text
    /// wheels-v0
    /// ├── pypi
    /// │   ├── ...
    /// │   ├── pandas-2.1.3-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.json
    /// │   ├── pandas-2.1.3-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl
    /// │   │   ├── pandas
    /// │   │   │   ├── ...
    /// │   │   ├── pandas-2.1.3.dist-info
    /// │   │   │   ├── ...
    /// │   │   └── pandas.libs
    /// │   ├── ...
    /// └── url
    ///     └── 4b8be67c801a7ecb
    ///         ├── flask-3.0.0-py3-none-any.json
    ///         └── flask-3.0.0-py3-none-any.whl
    ///             ├── flask
    ///             │   └── ...
    ///             └── flask-3.0.0.dist-info
    ///                 └── ...
    Wheels,
    /// Wheels built from source distributions, their extracted metadata and the cache policy of
    /// the source distribution.
    ///
    /// The structure is similar of that of the `Wheel` bucket, except we have an additional layer
    /// for the source dist filename and the metadata is on the source dist level, not on the wheel
    /// level.
    ///
    /// TODO(konstin): The cache policy should be on the source dist level, the metadata we can put
    /// next to the wheels as in the `Wheels` bucket.
    ///
    /// When resolving, we only build the wheel and store the archive file in the cache, when
    /// installing, we unpack it and store it as directory under the same name, so it's a mix of
    /// wheel archive zip files and unzipped wheel directories in the cache.
    ///
    /// Cache structure:
    ///  * `built-wheels-v0/pypi/foo-1.0.0.zip/{metadata.json, foo-1.0.0-py3-none-any.whl, ...other wheels}`
    ///  * `built-wheels-v0/<digest(index-url)>/foo-1.0.0.zip/{metadata.json, foo-1.0.0-py3-none-any.whl, ...other wheels}`
    ///  * `built-wheels-v0/url/<digest(url)>/foo-1.0.0.zip/{metadata.json, foo-1.0.0-py3-none-any.whl, ...other wheels}`
    ///  * `built-wheels-v0/git/<digest(url)>/<git sha>/foo-1.0.0.zip/{metadata.json, foo-1.0.0-py3-none-any.whl, ...other wheels}`
    ///
    /// But the url filename does not need to be a valid source dist filename
    /// (<https://github.com/search?q=path%3A**%2Frequirements.txt+master.zip&type=code>),
    /// so it could also be the following and we have to take any string as filename:
    ///  * `built-wheels-v0/url/<sha256(url)>/master.zip/metadata.json`
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
    /// built-wheels-v0/
    /// ├── git
    /// │   └── a67db8ed076e3814
    /// │       └── 843b753e9e8cb74e83cac55598719b39a4d5ef1f
    /// │           ├── metadata.json
    /// │           └── pydantic_extra_types-2.1.0-py3-none-any.whl
    /// ├── pypi
    /// │   └── django-allauth-0.51.0.tar.gz
    /// │       ├── django_allauth-0.51.0-py3-none-any.whl
    /// │       └── metadata.json
    /// └── url
    ///     └── 6781bd6440ae72c2
    ///         └── werkzeug-3.0.1.tar.gz
    ///             ├── metadata.json
    ///             └── werkzeug-3.0.1-py3-none-any.whl
    /// ```
    ///
    /// The inside of a `metadata.json`:
    /// ```json
    /// {
    ///   "data": {
    ///     "django_allauth-0.51.0-py3-none-any.whl": {
    ///       "metadata-version": "2.1",
    ///       "name": "django-allauth",
    ///       "version": "0.51.0",
    ///       ...
    ///     }
    ///   }
    /// }
    /// ```
    BuiltWheels,
    /// Git repositories.
    Git,
    /// Information about an interpreter at a path.
    ///
    /// To avoid caching pyenv shims, bash scripts which may redirect to a new python version
    /// without the shim itself changing, we only cache when the path equals `sys.executable`, i.e.
    /// the path we're running is the python executable itself and not a shim.
    ///
    /// Cache structure: `interpreter-v0/<digest(path)>.json`
    ///
    /// # Example
    ///
    /// The contents of each of the json files has a timestamp field in unix time, the [PEP 508]
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
    ///     "sys_executable": "/home/ferris/projects/puffin/.venv/bin/python"
    ///   }
    /// }
    /// ```
    ///
    /// [PEP 508]: https://peps.python.org/pep-0508/#environment-markers
    Interpreter,
    /// Index responses through the simple metadata API.
    ///
    /// Cache structure:
    ///  * `simple-v0/pypi/<package_name>.json`
    ///  * `simple-v0/<digest(index_url)>/<package_name>.json`
    ///
    /// TODO(konstin): Link to the json type after <https://github.com/astral-sh/puffin/pull/522/>
    Simple,
}

impl CacheBucket {
    fn to_str(self) -> &'static str {
        match self {
            CacheBucket::BuiltWheels => "built-wheels-v0",
            CacheBucket::Git => "git-v0",
            CacheBucket::Interpreter => "interpreter-v0",
            CacheBucket::Simple => "simple-v0",
            CacheBucket::Wheels => "wheels-v0",
        }
    }
}

impl Display for CacheBucket {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.to_str())
    }
}
