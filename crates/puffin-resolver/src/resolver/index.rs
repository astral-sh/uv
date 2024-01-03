use url::Url;

use distribution_types::{IndexUrl, PackageId};
use pep440_rs::VersionSpecifiers;
use puffin_normalize::PackageName;
use puffin_traits::OnceMap;
use pypi_types::{BaseUrl, Metadata21};

use crate::version_map::VersionMap;

/// In-memory index of package metadata.
#[derive(Default)]
pub(crate) struct Index {
    /// A map from package name to the metadata for that package and the index where the metadata
    /// came from.
    pub(crate) packages: OnceMap<PackageName, (IndexUrl, BaseUrl, VersionMap)>,

    /// A map from package ID to metadata for that distribution.
    pub(crate) distributions: OnceMap<PackageId, Metadata21>,

    /// A map from package ID to required Python version.
    pub(crate) incompatibilities: OnceMap<PackageId, VersionSpecifiers>,

    /// A map from source URL to precise URL.
    pub(crate) redirects: OnceMap<Url, Url>,
}

impl Index {
    /// Cancel all waiting tasks.
    ///
    /// Warning: waiting on tasks that have been canceled will cause the index to hang.
    pub(crate) fn cancel_all(&self) {
        self.packages.cancel_all();
        self.distributions.cancel_all();
        self.incompatibilities.cancel_all();
        self.redirects.cancel_all();
    }
}
