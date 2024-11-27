use std::sync::Arc;

use rustc_hash::{FxHashMap, FxHashSet};

use uv_normalize::PackageName;
use uv_pep440::Version;
use uv_pypi_types::RequirementSource;

use crate::{DependencyMode, Manifest, ResolverEnvironment};

/// A set of package versions that are permitted, even if they're marked as yanked by the
/// relevant index.
#[derive(Debug, Default, Clone)]
pub struct AllowedYanks(Arc<FxHashMap<PackageName, FxHashSet<Version>>>);

impl AllowedYanks {
    pub fn from_manifest(
        manifest: &Manifest,
        env: &ResolverEnvironment,
        dependencies: DependencyMode,
    ) -> Self {
        let mut allowed_yanks = FxHashMap::<PackageName, FxHashSet<Version>>::default();

        // Allow yanks for any pinned input requirements.
        for requirement in manifest.requirements(env, dependencies) {
            let RequirementSource::Registry { specifier, .. } = &requirement.source else {
                continue;
            };
            let [specifier] = specifier.as_ref() else {
                continue;
            };
            if matches!(
                specifier.operator(),
                uv_pep440::Operator::Equal | uv_pep440::Operator::ExactEqual
            ) {
                allowed_yanks
                    .entry(requirement.name.clone())
                    .or_default()
                    .insert(specifier.version().clone());
            }
        }

        // Allow yanks for any packages that are already pinned in the lockfile.
        for (name, preferences) in manifest.preferences.iter() {
            allowed_yanks
                .entry(name.clone())
                .or_default()
                .extend(preferences.map(|(.., version)| version.clone()));
        }

        Self(Arc::new(allowed_yanks))
    }

    /// Returns `true` if the package-version is allowed, even if it's marked as yanked.
    pub fn contains(&self, package_name: &PackageName, version: &Version) -> bool {
        self.0
            .get(package_name)
            .is_some_and(|versions| versions.contains(version))
    }
}
