use std::path::PathBuf;

use url::Url;

use pypi_types::IndexUrl;

use crate::{digest, CanonicalUrl};

/// Cache for metadata from (remote) wheels files
const WHEEL_METADATA_CACHE: &str = "wheel-metadata-v0";
/// Cache for metadata from wheels build from source dists
const BUILT_WHEEL_METADATA_CACHE: &str = "built-wheel-metadata-v0";

/// Cache wheel metadata, both from remote wheels and built from source distributions.
///
/// See [`WheelMetadataCache::wheel_dir`] for remote wheel metadata caching and
/// [`WheelMetadataCache::built_wheel_dir`] for caching of metadata of built source
/// distributions.
pub enum WheelMetadataCache<'a> {
    /// Either pypi or an alternative index, which we key by index url
    Index(&'a IndexUrl),
    /// A direct url dependency, which we key by url
    Url(&'a Url),
    /// A git dependency, which we key by repository url. We use the revision as filename.
    ///
    /// Note that this variant only exists for source distributions, wheels can't be delivered
    /// through git.
    Git(&'a Url),
}

impl<'a> WheelMetadataCache<'a> {
    fn bucket(&self) -> PathBuf {
        match self {
            WheelMetadataCache::Index(IndexUrl::Pypi) => PathBuf::from("pypi"),
            WheelMetadataCache::Index(url) => {
                PathBuf::from("index").join(digest(&CanonicalUrl::new(url)))
            }
            WheelMetadataCache::Url(url) => {
                PathBuf::from("url").join(digest(&CanonicalUrl::new(url)))
            }
            WheelMetadataCache::Git(url) => {
                PathBuf::from("git").join(digest(&CanonicalUrl::new(url)))
            }
        }
    }

    /// Metadata of a remote wheel
    ///
    /// Cache structure:
    ///  * `<wheel metadata cache>/pypi/foo-1.0.0-py3-none-any.json`
    ///  * `<wheel metadata cache>/<digest(index-url)>/foo-1.0.0-py3-none-any.json`
    ///  * `<wheel metadata cache>/url/<digest(url)>/foo-1.0.0-py3-none-any.json`
    ///
    /// See `puffin_client::RegistryClient::wheel_metadata` for information on how wheel metadata
    /// is fetched.
    ///
    /// # Example
    /// ```text
    /// # pypi wheel
    /// pandas
    /// # url wheel
    /// flask @ https://files.pythonhosted.org/packages/36/42/015c23096649b908c809c69388a805a571a3bea44362fe87e33fc3afa01f/flask-3.0.0-py3-none-any.whl
    /// ```
    /// may be cached as
    /// ```text
    /// wheel-metadata-v0
    /// ├── pypi
    /// │   ...
    /// │   ├── pandas-2.1.3-cp310-cp310-manylinux_2_17_x86_64.manylinux2014_x86_64.json
    /// │   ...
    /// └── url
    ///     └── 4b8be67c801a7ecb
    ///         └── flask-3.0.0-py3-none-any.json
    /// ```
    pub fn wheel_dir(&self) -> PathBuf {
        PathBuf::from(WHEEL_METADATA_CACHE).join(self.bucket())
    }

    /// Metadata of a built source distribution
    ///
    /// Cache structure:
    ///  * `<build wheel metadata cache>/pypi/foo-1.0.0.zip/metadata.json`
    ///  * `<build wheel metadata cache>/<sha256(index-url)>/foo-1.0.0.zip/metadata.json`
    ///  * `<build wheel metadata cache>/url/<sha256(url)>/foo-1.0.0.zip/metadata.json`
    /// But the url filename does not need to be a valid source dist filename
    /// (<https://github.com/search?q=path%3A**%2Frequirements.txt+master.zip&type=code>),
    /// so it could also be the following and we have to take any string as filename:
    ///  * `<build wheel metadata cache>/url/<sha256(url)>/master.zip/metadata.json`
    ///
    /// # Example
    /// ```text
    /// # git source dist
    /// pydantic-extra-types @ git+https://github.com/pydantic/pydantic-extra-types.git    
    /// # pypi source dist
    /// django_allauth==0.51.0
    /// # url source dist
    /// werkzeug @ https://files.pythonhosted.org/packages/0d/cc/ff1904eb5eb4b455e442834dabf9427331ac0fa02853bf83db817a7dd53d/werkzeug-3.0.1.tar.gz
    /// ```
    /// may be cached as
    /// ```text
    /// built-wheel-metadata-v0
    /// ├── git
    /// │   └── 5c56bc1c58c34c11
    /// │       └── 843b753e9e8cb74e83cac55598719b39a4d5ef1f
    /// │           └── metadata.json
    /// ├── pypi
    /// │   └── django-allauth-0.51.0.tar.gz
    /// │       └── metadata.json
    /// └── url
    ///     └── 6781bd6440ae72c2
    ///         └── werkzeug-3.0.1.tar.gz
    ///             └── metadata.json
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
    pub fn built_wheel_dir(&self, filename: &str) -> PathBuf {
        PathBuf::from(BUILT_WHEEL_METADATA_CACHE)
            .join(self.bucket())
            .join(filename)
    }
}
