use pypi_types::RequirementSource;
use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::Arc;

use pep440_rs::Version;
use pep508_rs::MarkerEnvironment;
use uv_normalize::PackageName;

use crate::{DependencyMode, Manifest};

/// A set of package versions that are permitted, even if they're marked as yanked by the
/// relevant index.
#[derive(Debug, Default, Clone)]
pub struct AllowedYanks(Arc<FxHashMap<PackageName, FxHashSet<Version>>>);

impl AllowedYanks {
    pub fn from_manifest(
        manifest: &Manifest,
        markers: Option<&MarkerEnvironment>,
        dependencies: DependencyMode,
    ) -> Self {
        let mut allowed_yanks = FxHashMap::<PackageName, FxHashSet<Version>>::default();

        // Allow yanks for any pinned input requirements.
        for requirement in manifest.requirements(markers, dependencies) {
            let RequirementSource::Registry { specifier, .. } = &requirement.source else {
                continue;
            };
            let [specifier] = specifier.as_ref() else {
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

        // Allow yanks for any packages that are already pinned in the lockfile.
        for (name, version) in manifest.preferences.iter() {
            allowed_yanks
                .entry(name.clone())
                .or_default()
                .insert(version.clone());
        }

        Self(Arc::new(allowed_yanks))
    }

    /// Returns `true` if the package-version is allowed, even if it's marked as yanked.
    pub fn contains(&self, package_name: &PackageName, version: &Version) -> bool {
        self.0
            .get(package_name)
            .map_or(false, |versions| versions.contains(version))
    }
}
