use std::sync::Arc;

use rustc_hash::FxHashMap;

use uv_distribution_types::{Requirement, RequirementSource};
use uv_normalize::PackageName;
use uv_pep440::Version;
use uv_pep508::MarkerTree;

use crate::resolver::ForkSet;
use crate::{DependencyMode, Manifest, ResolverEnvironment};

/// A set of package versions that are permitted, even if they're marked as yanked by the
/// relevant index.
#[derive(Debug, Default, Clone)]
pub struct AllowedYanks(Arc<FxHashMap<PackageName, FxHashMap<Version, MarkerTree>>>);

impl AllowedYanks {
    /// Allow an explicitly pinned yank from a selected first-party candidate.
    pub(crate) fn register(&mut self, requirement: &Requirement) {
        if let Some(version) = Self::explicit_pin(requirement) {
            self.register_marker(
                &requirement.name,
                version,
                ForkSet::requirement_marker(requirement),
            );
        }
    }

    pub(crate) fn register_version(&mut self, name: &PackageName, version: &Version) {
        self.register_marker(name, version, MarkerTree::TRUE);
    }

    fn register_marker(&mut self, name: &PackageName, version: &Version, marker: MarkerTree) {
        Arc::make_mut(&mut self.0)
            .entry(name.clone())
            .or_default()
            .entry(version.clone())
            .and_modify(|existing| *existing = existing.or(marker))
            .or_insert(marker);
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
        let mut allowed_yanks = Self::default();

        // Allow yanks for any pinned input requirements.
        for requirement in manifest.candidate_selection_requirements(env, dependencies) {
            allowed_yanks.register(&requirement);
        }

        // Allow yanks for any packages that are already pinned in the lockfile.
        for (name, preferences) in manifest.preferences.iter() {
            for (.., version) in preferences {
                allowed_yanks.register_version(name, version);
            }
        }

        allowed_yanks
    }

    /// Returns `true` if the package-version is allowed, even if it's marked as yanked.
    pub(crate) fn contains(&self, package_name: &PackageName, version: &Version) -> bool {
        !self.marker(package_name, version).is_false()
    }

    pub(crate) fn contains_in(
        &self,
        package_name: &PackageName,
        version: &Version,
        env: &ResolverEnvironment,
    ) -> bool {
        let marker = self.marker(package_name, version);
        env.marker_environment().map_or_else(
            || env.included_by_marker(marker),
            |environment| marker.without_extras().evaluate(environment, &[]),
        )
    }

    pub(crate) fn marker(&self, package_name: &PackageName, version: &Version) -> MarkerTree {
        self.0
            .get(package_name)
            .and_then(|versions| versions.get(version))
            .copied()
            .unwrap_or(MarkerTree::FALSE)
    }

    pub(crate) fn versions(&self, package_name: &PackageName) -> Vec<Version> {
        let mut versions = self
            .0
            .get(package_name)
            .into_iter()
            .flat_map(|versions| versions.keys())
            .cloned()
            .collect::<Vec<_>>();
        versions.sort_unstable();
        versions
    }
}
