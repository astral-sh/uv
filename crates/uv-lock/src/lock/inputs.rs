use std::collections::BTreeSet;

use uv_configuration::{Constraint, ExcludeDependency, Override};
use uv_distribution_types::{DependencyMetadata, ResolutionInputs};

use super::ResolverManifest;

impl ResolverManifest {
    /// Retain the configuration consulted by a runtime resolution, including unsuccessful lookups.
    ///
    /// The same projection is used when writing and validating a lock. A newly added declaration
    /// that matches a recorded lookup therefore invalidates the lock, even if nothing matched before.
    /// Locks without a recorded trace continue to compare the entire configuration.
    #[must_use]
    pub fn with_resolution_inputs(mut self, inputs: Option<ResolutionInputs>) -> Self {
        if let Some(inputs) = &inputs {
            // Scoped constraints and overrides also affect global candidate selection. Retain all
            // scopes (including empty ones) for a parent with a declaration for a consulted name.
            // Its exclusions can suppress those contributions, even if the parent never resolves.
            let mut packages = inputs.packages.clone();
            for entry in &self.constraints {
                if let Constraint::Package(scope) = entry
                    && scope
                        .dependencies
                        .iter()
                        .any(|requirement| inputs.requirements.contains(&requirement.name))
                {
                    packages.insert(scope.package.name().clone());
                }
            }
            for entry in &self.overrides {
                if let Override::Package(scope) = entry
                    && scope
                        .dependencies
                        .iter()
                        .any(|requirement| inputs.requirements.contains(&requirement.name))
                {
                    packages.insert(scope.package.name().clone());
                }
            }
            self.constraints.retain(|entry| match entry {
                Constraint::Requirement(requirement) => {
                    inputs.requirements.contains(&requirement.name)
                }
                Constraint::Package(scope) => packages.contains(scope.package.name()),
            });
            self.overrides.retain(|entry| match entry {
                Override::Requirement(requirement) => {
                    inputs.requirements.contains(&requirement.name)
                }
                Override::Package(scope) => packages.contains(scope.package.name()),
            });
            self.excludes.retain(|entry| match entry {
                ExcludeDependency::Dependency(name) => inputs.requirements.contains(name),
                ExcludeDependency::Package(scope) => packages.contains(scope.package()),
            });

            let metadata =
                DependencyMetadata::from_entries(self.dependency_metadata.iter().cloned());
            let selected = inputs
                .dependency_metadata
                .iter()
                .filter_map(|query| {
                    let version = query.version.as_ref()?;
                    let entry = metadata.get_entry(&query.name, Some(version))?;
                    Some((&entry.name, &entry.version))
                })
                .collect::<BTreeSet<_>>();
            // Unknown-version lookups depend on the number of declarations. Retain all entries
            // for those names, even when the lookup returned no metadata.
            let unversioned = inputs
                .dependency_metadata
                .iter()
                .filter(|query| query.version.is_none())
                .map(|query| &query.name)
                .collect::<BTreeSet<_>>();
            self.dependency_metadata.retain(|entry| {
                selected.contains(&(&entry.name, &entry.version))
                    || unversioned.contains(&entry.name)
            });
        }
        self.resolution_inputs = inputs;
        self
    }
}
