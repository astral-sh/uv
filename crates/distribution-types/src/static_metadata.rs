use pep440_rs::Version;
use pypi_types::Metadata23;
use rustc_hash::FxHashMap;
use uv_normalize::PackageName;

/// Pre-defined [`Metadata23`] entries, indexed by [`PackageName`] and [`Version`].
#[derive(Debug, Clone, Default)]
pub struct StaticMetadata(FxHashMap<PackageName, FxHashMap<Version, Metadata23>>);

impl StaticMetadata {
    /// Index a set of [`Metadata23`] entries by [`PackageName`] and [`Version`].
    pub fn from_entries(entries: impl IntoIterator<Item = Metadata23>) -> Self {
        let mut map = Self::default();
        for entry in entries {
            map.0
                .entry(entry.name.clone())
                .or_default()
                .insert(entry.version.clone(), entry);
        }
        map
    }

    /// Retrieve a [`Metadata23`] entry by [`PackageName`] and [`Version`].
    pub fn get(&self, package: &PackageName, version: &Version) -> Option<&Metadata23> {
        self.0.get(package).and_then(|map| map.get(version))
    }

    /// Retrieve all [`Metadata23`] entries.
    pub fn values(&self) -> impl Iterator<Item = &Metadata23> {
        self.0.values().flat_map(|map| map.values())
    }
}
