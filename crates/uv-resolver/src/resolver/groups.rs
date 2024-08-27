use rustc_hash::FxHashMap;

use uv_normalize::{GroupName, PackageName};

use crate::{Manifest, ResolverMarkers};

/// A map of package names to their activated dependency groups.
#[derive(Debug, Default, Clone)]
pub(crate) struct Groups(FxHashMap<PackageName, Vec<GroupName>>);

impl Groups {
    /// Determine the set of enabled dependency groups in the [`Manifest`].
    pub(crate) fn from_manifest(manifest: &Manifest, markers: &ResolverMarkers) -> Self {
        let mut groups = FxHashMap::default();

        // Enable the groups for all direct dependencies. In practice, this tends to mean: when
        // development dependencies are enabled, enable them for all direct dependencies.
        for group in &manifest.dev {
            for requirement in manifest.direct_requirements(markers) {
                groups
                    .entry(requirement.name.clone())
                    .or_insert_with(Vec::new)
                    .push(group.clone());
            }
        }

        Self(groups)
    }

    /// Retrieve the enabled dependency groups for a given package.
    pub(crate) fn get(&self, package: &PackageName) -> Option<&[GroupName]> {
        self.0.get(package).map(Vec::as_slice)
    }
}
