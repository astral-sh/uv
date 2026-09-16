use rustc_hash::FxHashMap;

use uv_normalize::PackageName;
use uv_pypi_types::VerbatimParsedUrl;

/// Direct source presentations used to explain a failed solver branch.
#[derive(Default, Debug, Clone)]
pub(crate) struct ForkUrls(FxHashMap<PackageName, VerbatimParsedUrl>);

impl ForkUrls {
    /// Whether we use a URL for this package.
    pub(crate) fn contains_key(&self, package_name: &PackageName) -> bool {
        self.0.contains_key(package_name)
    }
}

impl FromIterator<(PackageName, VerbatimParsedUrl)> for ForkUrls {
    fn from_iter<T: IntoIterator<Item = (PackageName, VerbatimParsedUrl)>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}
