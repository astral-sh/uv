use rustc_hash::{FxHashMap, FxHashSet};

use pep440_rs::Version;
use pep508_rs::{MarkerEnvironment, VersionOrUrl};
use uv_normalize::PackageName;

use crate::{Manifest, Preference};

/// A set of package versions that are permitted, even if they're marked as yanked by the
/// relevant index.
#[derive(Debug, Default, Clone)]
pub struct AllowedYanks(FxHashMap<PackageName, FxHashSet<Version>>);

impl AllowedYanks {
    pub fn from_manifest(manifest: &Manifest, markers: &MarkerEnvironment) -> Self {
        let mut allowed_yanks = FxHashMap::<PackageName, FxHashSet<Version>>::default();
        for requirement in manifest
            .requirements(markers)
            .chain(manifest.preferences.iter().map(Preference::requirement))
        {
            let Some(VersionOrUrl::VersionSpecifier(specifiers)) = &requirement.version_or_url
            else {
                continue;
            };
            let [specifier] = specifiers.as_ref() else {
                continue;
            };
            if matches!(
                specifier.operator(),
                pep440_rs::Operator::Equal | pep440_rs::Operator::ExactEqual
            ) {
                allowed_yanks
                    .entry(requirement.name.clone())
                    .or_default()
                    .insert(specifier.version().clone());
            }
        }
        Self(allowed_yanks)
    }

    /// Returns versions for the given package which are allowed even if marked as yanked by the
    /// relevant index.
    pub fn allowed_versions(&self, package_name: &PackageName) -> Option<&FxHashSet<Version>> {
        self.0.get(package_name)
    }
}
