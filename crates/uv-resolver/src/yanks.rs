use pypi_types::RequirementSource;
use rustc_hash::{FxHashMap, FxHashSet};

use pep440_rs::Version;
use pep508_rs::MarkerEnvironment;
use uv_normalize::PackageName;

use crate::{DependencyMode, Manifest};

/// A set of package versions that are permitted, even if they're marked as yanked by the
/// relevant index.
#[derive(Debug, Default, Clone)]
pub struct AllowedYanks(FxHashMap<PackageName, FxHashSet<Version>>);

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
        for preference in &manifest.preferences {
            allowed_yanks
                .entry(preference.name().clone())
                .or_default()
                .insert(preference.version().clone());
        }

        Self(allowed_yanks)
    }

    /// Returns versions for the given package which are allowed even if marked as yanked by the
    /// relevant index.
    pub fn allowed_versions(&self, package_name: &PackageName) -> Option<&FxHashSet<Version>> {
        self.0.get(package_name)
    }
}
