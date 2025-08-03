use std::collections::BTreeMap;

use uv_normalize::PackageName;

use crate::Requirement;

/// Lowered extra build dependencies with source resolution applied.
#[derive(Debug, Clone, Default)]
pub struct ExtraBuildRequires(BTreeMap<PackageName, Vec<Requirement>>);

impl std::ops::Deref for ExtraBuildRequires {
    type Target = BTreeMap<PackageName, Vec<Requirement>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for ExtraBuildRequires {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl IntoIterator for ExtraBuildRequires {
    type Item = (PackageName, Vec<Requirement>);
    type IntoIter = std::collections::btree_map::IntoIter<PackageName, Vec<Requirement>>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl FromIterator<(PackageName, Vec<Requirement>)> for ExtraBuildRequires {
    fn from_iter<T: IntoIterator<Item = (PackageName, Vec<Requirement>)>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}
