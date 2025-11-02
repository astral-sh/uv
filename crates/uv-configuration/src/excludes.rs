use rustc_hash::FxHashSet;

use uv_normalize::PackageName;

/// A set of packages to exclude from resolution.
#[derive(Debug, Default, Clone)]
pub struct Excludes(FxHashSet<PackageName>);

impl Excludes {
    /// Return an iterator over all package names in the exclusion set.
    pub fn iter(&self) -> impl Iterator<Item = &PackageName> {
        self.0.iter()
    }

    /// Check if a package is excluded.
    pub fn contains(&self, name: &PackageName) -> bool {
        self.0.contains(name)
    }
}

impl FromIterator<PackageName> for Excludes {
    fn from_iter<I: IntoIterator<Item = PackageName>>(iter: I) -> Self {
        Self(iter.into_iter().collect())
    }
}
