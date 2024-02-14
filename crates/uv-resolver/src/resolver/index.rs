use dashmap::DashMap;
use url::Url;

use distribution_types::PackageId;
use once_map::OnceMap;
use pypi_types::Metadata21;
use uv_normalize::PackageName;

use super::provider::VersionsResponse;

/// In-memory index of package metadata.
#[derive(Default)]
pub struct InMemoryIndex {
    /// A map from package name to the metadata for that package and the index where the metadata
    /// came from.
    pub(crate) packages: OnceMap<PackageName, VersionsResponse>,

    /// A map from package ID to metadata for that distribution.
    pub(crate) distributions: OnceMap<PackageId, Metadata21>,

    /// A map from source URL to precise URL. For example, the source URL
    /// `git+https://github.com/pallets/flask.git` could be redirected to
    /// `git+https://github.com/pallets/flask.git@c2f65dd1cfff0672b902fd5b30815f0b4137214c`.
    pub(crate) redirects: DashMap<Url, Url>,
}
