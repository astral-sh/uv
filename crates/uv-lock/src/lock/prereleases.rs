use std::collections::BTreeSet;

use uv_configuration::{
    ExcludeDependency, Excludes, Override, Overrides, PrereleaseMode,
    specifier_opts_into_prereleases,
};
use uv_distribution_types::{Requirement, RequirementSource};
use uv_normalize::PackageName;
use uv_pep440::Version;

use super::{Lock, LockError, LockErrorKind, Package, Source};

/// The prerequisites for omitting constraints that opt into prereleases.
pub(super) struct PrereleaseConstraints {
    simple: bool,
    conditional: BTreeSet<PackageName>,
}

impl PrereleaseConstraints {
    pub(super) fn new<'a>(lock: &Lock, constraints: impl Iterator<Item = &'a Requirement>) -> Self {
        if !lock.has_explicit_registry_prerelease() || lock.simple_prerelease_root().is_none() {
            return Self {
                simple: false,
                conditional: BTreeSet::new(),
            };
        }
        let conditional = constraints
            .filter(|requirement| {
                opts_into_prereleases(requirement) && !is_unconditional(requirement)
            })
            .map(|requirement| requirement.name.clone())
            .collect();
        Self {
            simple: true,
            conditional,
        }
    }

    pub(super) fn can_omit(&self, name: &PackageName) -> bool {
        self.simple && !self.conditional.contains(name)
    }
}

/// Return whether a registry requirement opts into prerelease candidates.
fn opts_into_prereleases(requirement: &Requirement) -> bool {
    let RequirementSource::Registry { specifier, .. } = &requirement.source else {
        return false;
    };
    specifier.iter().any(specifier_opts_into_prereleases)
}

/// Return whether a requirement applies without a marker or conflict scope.
fn is_unconditional(requirement: &Requirement) -> bool {
    let RequirementSource::Registry { conflict, .. } = &requirement.source else {
        return false;
    };
    requirement.marker.is_true()
        && conflict.is_none()
        && requirement.scope.conflict_item().is_none()
}

impl Lock {
    /// Return whether a locked registry prerelease uses the explicit selection policy.
    fn has_explicit_registry_prerelease(&self) -> bool {
        if self.options.prerelease.global != PrereleaseMode::Explicit
            && !self
                .options
                .prerelease
                .package
                .values()
                .any(|mode| *mode == PrereleaseMode::Explicit)
        {
            return false;
        }
        self.packages.iter().any(|package| {
            matches!(package.id.source, Source::Registry(_))
                && package
                    .id
                    .version
                    .as_ref()
                    .is_some_and(Version::any_prerelease)
                && self.options.prerelease.mode(&package.id.name) == PrereleaseMode::Explicit
        })
    }

    /// Return the root when its metadata contains all first-party prerelease declarations.
    ///
    /// External sources can introduce additional declarations through lookahead. Metadata-free
    /// locks and dynamic roots cannot establish the declarations from the stored metadata.
    fn simple_prerelease_root(&self) -> Option<&Package> {
        if self.supports_missing_package_metadata()
            || self.packages.iter().any(|package| {
                !self.is_workspace_package(package)
                    && !matches!(package.id.source, Source::Registry(_))
            })
        {
            return None;
        }
        let root = self.simple_root()?;
        if !root.has_metadata()
            || root.is_dynamic()
            || !root.metadata.dependency_groups.is_empty()
            || root.metadata.requires_dist.iter().any(|requirement| {
                !requirement.extras.is_empty()
                    || !requirement.groups.is_empty()
                    || !is_unconditional(requirement)
            })
        {
            return None;
        }
        Some(root)
    }

    /// Find a locked registry prerelease that no current declaration opts into.
    ///
    /// Call after validating the root metadata, so its declarations are known to match the
    /// current workspace. Constraints with conditional opt-ins are checked by comparing their
    /// retained declarations instead.
    pub(super) fn unsatisfied_prerelease<'lock>(
        &'lock self,
        prereleases: &PrereleaseConstraints,
        constraints: &[Requirement],
        overrides: &[Override<Requirement>],
        excludes: &[ExcludeDependency],
    ) -> Result<Option<&'lock PackageName>, LockError> {
        if !prereleases.simple {
            return Ok(None);
        }
        let Some(root) = self.simple_prerelease_root() else {
            return Ok(None);
        };
        let Some(root_version) = root.id.version.as_ref() else {
            return Ok(None);
        };
        let overrides = Overrides::from_entries(overrides.to_vec())
            .map_err(LockErrorKind::InvalidScopedOverride)?;
        let excludes = Excludes::from_entries(excludes.iter().cloned());

        for package in &self.packages {
            if !prereleases.can_omit(&package.id.name)
                || self.options.prerelease.mode(&package.id.name) != PrereleaseMode::Explicit
                || !matches!(package.id.source, Source::Registry(_))
                || !package
                    .id
                    .version
                    .as_ref()
                    .is_some_and(Version::any_prerelease)
            {
                continue;
            }

            let is_opt_in = |requirement: &Requirement| {
                requirement.name == package.id.name
                    && is_unconditional(requirement)
                    && opts_into_prereleases(requirement)
            };
            let constraint_opt_in =
                !excludes.contains(&package.id.name) && constraints.iter().any(is_opt_in);
            let root_opt_in = overrides
                .apply_for(&root.id.name, root_version, &root.metadata.requires_dist)
                .filter(|requirement| {
                    !excludes.contains_for(&root.id.name, root_version, &requirement.name)
                })
                .any(|requirement| is_opt_in(&requirement));
            let global_override_opt_in = !excludes.contains(&package.id.name)
                && overrides.global_requirements().any(is_opt_in);
            let scoped_override_opt_in = overrides
                .scoped_requirements()
                .filter(|(name, version, requirement)| {
                    !excludes.contains_for_scope(&overrides, name, *version, &requirement.name)
                })
                .any(|(_, _, requirement)| is_opt_in(requirement));
            if !constraint_opt_in
                && !root_opt_in
                && !global_override_opt_in
                && !scoped_override_opt_in
            {
                return Ok(Some(&package.id.name));
            }
        }
        Ok(None)
    }
}
