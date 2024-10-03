use uv_distribution_types::IndexUrl;
use uv_normalize::PackageName;
use uv_pep508::VerbatimUrl;
use uv_pypi_types::RequirementSource;

use crate::resolver::ForkMap;
use crate::{DependencyMode, Manifest, ResolverMarkers};

/// A map of package names to their explicit index.
///
/// For example, given:
/// ```toml
/// [[tool.uv.index]]
/// name = "pytorch"
/// url = "https://download.pytorch.org/whl/cu121"
///
/// [tool.uv.sources]
/// torch = { index = "pytorch" }
/// ```
///
/// [`Indexes`] would contain a single entry mapping `torch` to `https://download.pytorch.org/whl/cu121`.
#[derive(Debug, Default, Clone)]
pub(crate) struct Indexes(ForkMap<IndexUrl>);

impl Indexes {
    /// Determine the set of explicit, pinned indexes in the [`Manifest`].
    pub(crate) fn from_manifest(
        manifest: &Manifest,
        markers: &ResolverMarkers,
        dependencies: DependencyMode,
    ) -> Self {
        let mut indexes = ForkMap::default();

        for requirement in manifest.requirements(markers, dependencies) {
            let RequirementSource::Registry {
                index: Some(index), ..
            } = &requirement.source
            else {
                continue;
            };
            let index = IndexUrl::from(VerbatimUrl::from_url(index.clone()));
            indexes.add(&requirement, index);
        }

        Self(indexes)
    }

    /// Returns `true` if the map contains any indexes for a package.
    pub(crate) fn contains_key(&self, name: &PackageName) -> bool {
        self.0.contains_key(name)
    }

    /// Return the explicit index used for a package in the given fork.
    pub(crate) fn get(
        &self,
        package_name: &PackageName,
        markers: &ResolverMarkers,
    ) -> Vec<&IndexUrl> {
        self.0.get(package_name, markers)
    }
}
