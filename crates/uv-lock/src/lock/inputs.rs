use std::collections::BTreeSet;

use uv_configuration::{Constraint, ExcludeDependency, Override};
use uv_distribution_types::{DependencyMetadata, Requirement, ResolutionLookups, StaticMetadata};
use uv_normalize::PackageName;
use uv_pep440::Version;

use super::ResolverManifest;

impl ResolverManifest {
    /// Store the runtime lookups and retain the configuration they consulted, including misses.
    ///
    /// Apply the same filter used when validating a lock. A newly added declaration that matches a
    /// recorded lookup therefore invalidates the lock, even if nothing matched before.
    /// Locks without a recorded trace continue to compare the entire configuration.
    #[must_use]
    pub fn prune_unused(mut self, lookups: Option<ResolutionLookups>) -> Self {
        if lookups.is_some() {
            let metadata =
                DependencyMetadata::from_entries(self.dependency_metadata.iter().cloned());
            let filter = ManifestFilter::new(
                lookups.as_ref(),
                &self.constraints,
                &self.overrides,
                &metadata,
            );
            self.constraints
                .retain(|entry| filter.includes_constraint(entry));
            self.overrides
                .retain(|entry| filter.includes_override(entry));
            self.excludes
                .retain(|entry| filter.includes_exclusion(entry));
            self.dependency_metadata
                .retain(|entry| filter.includes_metadata(entry));
        }
        self.resolution_inputs = lookups;
        self
    }
}

/// Select configuration relevant to recorded runtime lookups when writing or validating a lock.
pub(super) struct ManifestFilter<'a> {
    lookups: Option<&'a ResolutionLookups>,
    packages: BTreeSet<PackageName>,
    selected_metadata: BTreeSet<(&'a PackageName, &'a Option<Version>)>,
    unversioned_metadata: BTreeSet<&'a PackageName>,
}

impl<'a> ManifestFilter<'a> {
    /// Include all configuration when the lock has no recorded lookups.
    pub(super) fn new<'b>(
        lookups: Option<&'a ResolutionLookups>,
        constraints: impl IntoIterator<Item = &'b Constraint<Requirement>>,
        overrides: impl IntoIterator<Item = &'b Override<Requirement>>,
        metadata: &'a DependencyMetadata,
    ) -> Self {
        let mut filter = Self {
            lookups,
            packages: BTreeSet::new(),
            selected_metadata: BTreeSet::new(),
            unversioned_metadata: BTreeSet::new(),
        };
        let Some(lookups) = lookups else {
            return filter;
        };

        // Scoped constraints and overrides also affect global candidate selection. Retain all
        // scopes (including empty ones) for a parent with a declaration for a consulted name.
        // Its exclusions can suppress those contributions, even if the parent never resolves.
        filter.packages.clone_from(&lookups.packages);
        for entry in constraints {
            if let Constraint::Package(scope) = entry
                && scope
                    .dependencies
                    .iter()
                    .any(|requirement| lookups.requirements.contains(&requirement.name))
            {
                filter.packages.insert(scope.package.name().clone());
            }
        }
        for entry in overrides {
            if let Override::Package(scope) = entry
                && scope
                    .dependencies
                    .iter()
                    .any(|requirement| lookups.requirements.contains(&requirement.name))
            {
                filter.packages.insert(scope.package.name().clone());
            }
        }

        for query in &lookups.dependency_metadata {
            if let Some(version) = &query.version {
                if let Some(entry) = metadata.get_entry(&query.name, Some(version)) {
                    filter
                        .selected_metadata
                        .insert((&entry.name, &entry.version));
                }
            } else {
                // Unknown-version lookups depend on the number of declarations. Retain all entries
                // for those names, even when the lookup returned no metadata.
                filter.unversioned_metadata.insert(&query.name);
            }
        }
        filter
    }

    pub(super) fn includes_constraint(&self, entry: &Constraint<Requirement>) -> bool {
        self.lookups.is_none_or(|lookups| match entry {
            Constraint::Requirement(requirement) => {
                lookups.requirements.contains(&requirement.name)
            }
            Constraint::Package(scope) => self.packages.contains(scope.package.name()),
        })
    }

    pub(super) fn includes_override(&self, entry: &Override<Requirement>) -> bool {
        self.lookups.is_none_or(|lookups| match entry {
            Override::Requirement(requirement) => lookups.requirements.contains(&requirement.name),
            Override::Package(scope) => self.packages.contains(scope.package.name()),
        })
    }

    pub(super) fn includes_exclusion(&self, entry: &ExcludeDependency) -> bool {
        self.lookups.is_none_or(|lookups| match entry {
            ExcludeDependency::Dependency(name) => lookups.requirements.contains(name),
            ExcludeDependency::Package(scope) => self.packages.contains(scope.package()),
        })
    }

    pub(super) fn includes_metadata(&self, entry: &StaticMetadata) -> bool {
        self.lookups.is_none()
            || self
                .selected_metadata
                .contains(&(&entry.name, &entry.version))
            || self.unversioned_metadata.contains(&entry.name)
    }
}
