use rustc_hash::FxHashMap;
use pep440_rs::Version;
use pypi_types::Metadata23;
use uv_normalize::PackageName;

#[derive(Debug, Clone, Default)]
pub struct StaticMetadata(FxHashMap<PackageName, FxHashMap<Version, Metadata23>>);

impl StaticMetadata {
    pub fn insert(&mut self, package: PackageName, version: Version, metadata: Metadata23) {
        self.0.entry(package).or_default().insert(version, metadata);
    }

    pub fn get(&self, package: &PackageName, version: &Version) -> Option<&Metadata23> {
        self.0.get(package).and_then(|map| map.get(version))
    }
}
