use std::sync::Arc;

use rustc_hash::{FxHashMap, FxHashSet};

use uv_distribution_types::{Requirement, RequirementSource};
use uv_normalize::PackageName;
use uv_pep440::Version;

use crate::{DependencyMode, Manifest, ResolverEnvironment};

/// A set of package versions that are permitted, even if they're marked as yanked by the
/// relevant index.
#[derive(Debug, Default, Clone)]
pub struct AllowedYanks(Arc<FxHashMap<PackageName, FxHashSet<Version>>>);

impl AllowedYanks {
    /// Allow an explicitly pinned yank from a selected first-party candidate.
    pub(crate) fn register(&mut self, requirement: &Requirement) {
        if let Some(version) = Self::explicit_pin(requirement) {
            self.register_version(&requirement.name, version);
        }
    }

    pub(crate) fn register_version(&mut self, name: &PackageName, version: &Version) {
        Arc::make_mut(&mut self.0)
            .entry(name.clone())
            .or_default()
            .insert(version.clone());
    }

    pub(crate) fn explicit_pin(requirement: &Requirement) -> Option<&Version> {
        let RequirementSource::Registry { specifier, .. } = &requirement.source else {
            return None;
        };
        let [specifier] = specifier.as_ref() else {
            return None;
        };
        matches!(
            specifier.operator(),
            uv_pep440::Operator::Equal | uv_pep440::Operator::ExactEqual
        )
        .then_some(specifier.version())
    }

    pub fn from_manifest(
        manifest: &Manifest,
        env: &ResolverEnvironment,
        dependencies: DependencyMode,
    ) -> Self {
        let mut allowed_yanks = FxHashMap::<PackageName, FxHashSet<Version>>::default();

        // Allow yanks for any pinned input requirements.
        for requirement in manifest.candidate_selection_requirements(env, dependencies) {
            if let Some(version) = Self::explicit_pin(&requirement) {
                allowed_yanks
                    .entry(requirement.name.clone())
                    .or_default()
                    .insert(version.clone());
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
    pub(crate) fn contains(&self, package_name: &PackageName, version: &Version) -> bool {
        self.0
            .get(package_name)
            .is_some_and(|versions| versions.contains(version))
    }

    pub(crate) fn versions(&self, package_name: &PackageName) -> Vec<Version> {
        let mut versions = self
            .0
            .get(package_name)
            .into_iter()
            .flatten()
            .cloned()
            .collect::<Vec<_>>();
        versions.sort_unstable();
        versions
    }
}
