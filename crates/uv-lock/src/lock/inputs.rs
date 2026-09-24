use std::collections::BTreeSet;

use uv_configuration::{ExcludeDependency, Override};
use uv_distribution_types::{Requirement, ResolutionLookups, StaticMetadata};
use uv_normalize::PackageName;

use super::{Lock, Package};

impl Lock {
    /// Retain settings for locked packages and settings consulted during runtime resolution.
    ///
    /// Consulted settings can affect backtracking even when their packages are absent from the
    /// final graph. Retain complete parent scopes and metadata declarations for each relevant name.
    #[must_use]
    pub fn prune_unused(mut self, lookups: ResolutionLookups) -> Self {
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
                .chain(&filter.lookups.exclude_newer),
        );
        self
    }
}

/// Names whose settings participate in lockfile retention or validation.
///
/// Locked packages always participate. Other names participate separately for each setting, based
/// on recorded lookups when writing a lock and retained declarations when validating it.
pub(super) struct ManifestFilter {
    packages: BTreeSet<PackageName>,
    lookups: ResolutionLookups,
}

impl ManifestFilter {
    /// Select locked packages and settings consulted during resolution, including backtracking.
    fn from_resolution(lock: &Lock, mut lookups: ResolutionLookups) -> Self {
        // Scoped overrides also affect global candidate selection. Retain all
        // scopes (including empty ones) for a parent with a declaration for a consulted name.
        // Its exclusions can suppress those contributions, even if the parent never resolves.
        for entry in &lock.manifest.overrides {
            if let Override::Package(scope) = entry
                && scope
                    .dependencies
                    .iter()
                    .any(|requirement| lookups.candidate_policy.contains(&requirement.name))
            {
                lookups
                    .scoped_overrides
                    .insert(scope.package.name().clone());
                lookups
                    .scoped_exclusions
                    .insert(scope.package.name().clone());
            }
        }
        Self {
            packages: lock
                .packages
                .iter()
                .map(|package| package.name().clone())
                .collect(),
            lookups,
        }
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
            lookups: ResolutionLookups {
                constraints: lock
                    .manifest
                    .constraints
                    .iter()
                    .map(|requirement| requirement.name.clone())
                    .collect(),
                dependency_metadata: lock
                    .manifest
                    .dependency_metadata
                    .iter()
                    .map(|entry| entry.name.clone())
                    .collect(),
                ..ResolutionLookups::default()
            },
        };
        for entry in &lock.manifest.overrides {
            match entry {
                Override::Requirement(requirement) => {
                    filter.lookups.overrides.insert(requirement.name.clone());
                }
                Override::Package(scope) => {
                    filter
                        .lookups
                        .scoped_overrides
                        .insert(scope.package.name().clone());
                }
            }
        }
        for entry in &lock.manifest.excludes {
            match entry {
                ExcludeDependency::Dependency(name) => {
                    filter.lookups.exclusions.insert(name.clone());
                }
                ExcludeDependency::Package(scope) => {
                    filter
                        .lookups
                        .scoped_exclusions
                        .insert(scope.package().clone());
                }
            }
        }
        filter
    }

    pub(super) fn includes_constraint(&self, requirement: &Requirement) -> bool {
        self.packages.contains(&requirement.name)
            || self.lookups.constraints.contains(&requirement.name)
            || self.lookups.candidate_policy.contains(&requirement.name)
    }

    pub(super) fn includes_override(&self, entry: &Override<Requirement>) -> bool {
        match entry {
            Override::Requirement(requirement) => {
                self.packages.contains(&requirement.name)
                    || self.lookups.overrides.contains(&requirement.name)
                    || self.lookups.candidate_policy.contains(&requirement.name)
            }
            Override::Package(scope) => {
                self.packages.contains(scope.package.name())
                    || self.lookups.scoped_overrides.contains(scope.package.name())
            }
        }
    }

    pub(super) fn includes_exclusion(&self, entry: &ExcludeDependency) -> bool {
        match entry {
            ExcludeDependency::Dependency(name) => {
                self.packages.contains(name)
                    || self.lookups.exclusions.contains(name)
                    || self.lookups.candidate_policy.contains(name)
            }
            ExcludeDependency::Package(scope) => {
                self.packages.contains(scope.package())
                    || self.lookups.scoped_exclusions.contains(scope.package())
            }
        }
    }

    pub(super) fn includes_metadata(&self, entry: &StaticMetadata) -> bool {
        self.packages.contains(&entry.name)
            || self.lookups.dependency_metadata.contains(&entry.name)
    }
}
