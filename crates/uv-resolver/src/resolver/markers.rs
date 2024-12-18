use rustc_hash::FxHashMap;

use uv_normalize::PackageName;
use uv_pep508::MarkerTree;

/// The superset of markers for which a package is known to be relevant.
///
/// These markers may not represent the exact set of relevant environments, as they aren't adjusted
/// when backtracking; instead, we only _add_ to this set over the course of the resolution. As
/// such, the marker value represents a superset of the environments in which the package is known
/// to be included, but it may include environments in which the package is ultimately excluded.
#[derive(Debug, Default, Clone)]
pub(crate) struct KnownMarkers(FxHashMap<PackageName, MarkerTree>);

impl KnownMarkers {
    /// Inserts the given [`MarkerTree`] for the given package name.
    pub(crate) fn insert(&mut self, package_name: PackageName, marker_tree: MarkerTree) {
        self.0
            .entry(package_name)
            .or_insert(MarkerTree::FALSE)
            .or(marker_tree);
    }

    /// Returns the [`MarkerTree`] for the given package name, if it exists.
    pub(crate) fn get(&self, package_name: &PackageName) -> Option<&MarkerTree> {
        self.0.get(package_name)
    }
}
