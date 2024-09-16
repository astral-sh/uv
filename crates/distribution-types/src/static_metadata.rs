use pep440_rs::Version;
use pypi_types::Metadata23;
use rustc_hash::FxHashMap;
use uv_normalize::PackageName;

#[derive(Debug, Clone, Default)]
pub struct StaticMetadata(FxHashMap<PackageName, FxHashMap<Version, Metadata23>>);

impl StaticMetadata {
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

    pub fn insert(&mut self, package: PackageName, version: Version, metadata: Metadata23) {
        self.0.entry(package).or_default().insert(version, metadata);
    }

    pub fn get(&self, package: &PackageName, version: &Version) -> Option<&Metadata23> {
        self.0.get(package).and_then(|map| map.get(version))
    }
}
