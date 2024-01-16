use dashmap::DashMap;
use url::Url;

use distribution_types::PackageId;
use puffin_normalize::PackageName;
use puffin_traits::OnceMap;
use pypi_types::Metadata21;

use crate::version_map::VersionMap;

/// In-memory index of package metadata.
#[derive(Default)]
pub struct InMemoryIndex {
    /// A map from package name to the metadata for that package and the index where the metadata
    /// came from.
    pub(crate) packages: OnceMap<PackageName, VersionMap>,

    /// A map from package ID to metadata for that distribution.
    pub(crate) distributions: OnceMap<PackageId, Metadata21>,

    /// A map from source URL to precise URL. For example, the source URL
    /// `git+https://github.com/pallets/flask.git` could be redirected to
    /// `git+https://github.com/pallets/flask.git@c2f65dd1cfff0672b902fd5b30815f0b4137214c`.
    pub(crate) redirects: DashMap<Url, Url>,
}

impl InMemoryIndex {
    /// Cancel all waiting tasks.
    ///
    /// Warning: waiting on tasks that have been canceled will cause the index to hang.
    pub(crate) fn cancel_all(&self) {
        self.packages.cancel_all();
        self.distributions.cancel_all();
    }
}
