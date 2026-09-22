use std::collections::BTreeSet;

use uv_configuration::{Constraint, ExcludeDependency, Override};
use uv_distribution_types::{Requirement, ResolutionLookups, StaticMetadata};
use uv_normalize::PackageName;

use super::{Lock, Package};

impl Lock {
    /// Retain settings for locked packages and settings consulted during runtime resolution.
    ///
    /// Consulted settings can affect backtracking even when their packages are absent from the
    /// final graph. Retain complete parent scopes and metadata declarations for each relevant name.
    #[must_use]
    pub fn prune_unused(mut self, lookups: &ResolutionLookups) -> Self {
        let filter = ManifestFilter::from_resolution(&self, lookups);
        self.manifest
            .constraints
            .retain(|entry| filter.includes_constraint(entry));
        self.manifest
            .overrides
            .retain(|entry| filter.includes_override(entry));
        self.manifest
            .excludes
            .retain(|entry| filter.includes_exclusion(entry));
        self.manifest
            .dependency_metadata
            .retain(|entry| filter.includes_metadata(entry));
        self.options.exclude_newer = self.options.exclude_newer.filter_packages(
            self.packages
                .iter()
                .map(Package::name)
                .chain(&lookups.exclude_newer),
        );
        self
    }
}

/// Names whose settings participate in lockfile retention or validation.
///
/// Locked packages always participate. Other names participate separately for each setting, based
/// on runtime consultations when writing a lock and retained declarations when validating it.
#[derive(Default)]
pub(super) struct ManifestFilter {
    packages: BTreeSet<PackageName>,
    constraints: BTreeSet<PackageName>,
    overrides: BTreeSet<PackageName>,
    exclusions: BTreeSet<PackageName>,
    scoped_constraints: BTreeSet<PackageName>,
    scoped_overrides: BTreeSet<PackageName>,
    scoped_exclusions: BTreeSet<PackageName>,
    dependency_metadata: BTreeSet<PackageName>,
}

impl ManifestFilter {
    /// Select locked packages and settings consulted during resolution, including backtracking.
    fn from_resolution(lock: &Lock, lookups: &ResolutionLookups) -> Self {
        let mut filter = Self {
            packages: lock
                .packages
                .iter()
                .map(|package| package.name().clone())
                .collect(),
            constraints: lookups
                .constraints
                .union(&lookups.candidate_policy)
                .cloned()
                .collect(),
            overrides: lookups
                .overrides
                .union(&lookups.candidate_policy)
                .cloned()
                .collect(),
            exclusions: lookups
                .exclusions
                .union(&lookups.candidate_policy)
                .cloned()
                .collect(),
            scoped_constraints: lookups.scoped_constraints.clone(),
            scoped_overrides: lookups.scoped_overrides.clone(),
            scoped_exclusions: lookups.scoped_exclusions.clone(),
            dependency_metadata: lookups.dependency_metadata.clone(),
        };

        // Scoped constraints and overrides also affect global candidate selection. Retain all
        // scopes (including empty ones) for a parent with a declaration for a consulted name.
        // Its exclusions can suppress those contributions, even if the parent never resolves.
        for entry in &lock.manifest.constraints {
            if let Constraint::Package(scope) = entry
                && scope
                    .dependencies
                    .iter()
                    .any(|requirement| lookups.candidate_policy.contains(&requirement.name))
            {
                filter
                    .scoped_constraints
                    .insert(scope.package.name().clone());
                filter
                    .scoped_exclusions
                    .insert(scope.package.name().clone());
            }
        }
        for entry in &lock.manifest.overrides {
            if let Override::Package(scope) = entry
                && scope
                    .dependencies
                    .iter()
                    .any(|requirement| lookups.candidate_policy.contains(&requirement.name))
            {
                filter.scoped_overrides.insert(scope.package.name().clone());
                filter
                    .scoped_exclusions
                    .insert(scope.package.name().clone());
            }
        }
        filter
    }

    /// Compare settings for locked packages and previously retained keys.
    ///
    /// New settings outside this set take effect when another change triggers resolution. Keeping
    /// retained keys ensures changes and removals invalidate the lock even for backtracked packages.
    pub(super) fn from_lock(lock: &Lock) -> Self {
        let mut filter = Self {
            packages: lock
                .packages
                .iter()
                .map(|package| package.name().clone())
                .collect(),
            dependency_metadata: lock
                .manifest
                .dependency_metadata
                .iter()
                .map(|entry| entry.name.clone())
                .collect(),
            ..Self::default()
        };
        for entry in &lock.manifest.constraints {
            match entry {
                Constraint::Requirement(requirement) => {
                    filter.constraints.insert(requirement.name.clone());
                }
                Constraint::Package(scope) => {
                    filter
                        .scoped_constraints
                        .insert(scope.package.name().clone());
                }
            }
        }
        for entry in &lock.manifest.overrides {
            match entry {
                Override::Requirement(requirement) => {
                    filter.overrides.insert(requirement.name.clone());
                }
                Override::Package(scope) => {
                    filter.scoped_overrides.insert(scope.package.name().clone());
                }
            }
        }
        for entry in &lock.manifest.excludes {
            match entry {
                ExcludeDependency::Dependency(name) => {
                    filter.exclusions.insert(name.clone());
                }
                ExcludeDependency::Package(scope) => {
                    filter.scoped_exclusions.insert(scope.package().clone());
                }
            }
        }
        filter
    }

    pub(super) fn includes_constraint(&self, entry: &Constraint<Requirement>) -> bool {
        match entry {
            Constraint::Requirement(requirement) => {
                self.packages.contains(&requirement.name)
                    || self.constraints.contains(&requirement.name)
            }
            Constraint::Package(scope) => {
                self.packages.contains(scope.package.name())
                    || self.scoped_constraints.contains(scope.package.name())
            }
        }
    }

    pub(super) fn includes_override(&self, entry: &Override<Requirement>) -> bool {
        match entry {
            Override::Requirement(requirement) => {
                self.packages.contains(&requirement.name)
                    || self.overrides.contains(&requirement.name)
            }
            Override::Package(scope) => {
                self.packages.contains(scope.package.name())
                    || self.scoped_overrides.contains(scope.package.name())
            }
        }
    }

    pub(super) fn includes_exclusion(&self, entry: &ExcludeDependency) -> bool {
        match entry {
            ExcludeDependency::Dependency(name) => {
                self.packages.contains(name) || self.exclusions.contains(name)
            }
            ExcludeDependency::Package(scope) => {
                self.packages.contains(scope.package())
                    || self.scoped_exclusions.contains(scope.package())
            }
        }
    }

    pub(super) fn includes_metadata(&self, entry: &StaticMetadata) -> bool {
        self.packages.contains(&entry.name) || self.dependency_metadata.contains(&entry.name)
    }
}
