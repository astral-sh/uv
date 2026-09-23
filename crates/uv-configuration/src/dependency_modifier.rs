use std::borrow::Cow;

use uv_distribution_types::Requirement;
use uv_normalize::PackageName;
use uv_pep440::Version;

use crate::{Excludes, Overrides};

/// Overrides and exclusions to apply to dependency requirements.
#[derive(Debug, Default, Clone)]
pub struct DependencyModifiers {
    overrides: Overrides,
    excludes: Excludes,
}

/// The package scope in which to apply dependency modifiers.
#[derive(Debug, Clone, Copy)]
pub enum DependencyModifierScope<'a> {
    /// Apply global overrides and exclusions.
    Global,
    /// Apply global and package-scoped overrides and exclusions to regular package metadata.
    Package(&'a PackageName, &'a Version),
    /// Apply global modifiers and package-scoped exclusions to a dependency group.
    DependencyGroup(&'a PackageName, &'a Version),
}

impl DependencyModifiers {
    pub fn new(overrides: Overrides, excludes: Excludes) -> Self {
        Self {
            overrides,
            excludes,
        }
    }

    /// Return whether any modifiers are scoped to this package.
    pub fn has_scoped_package(&self, package: &PackageName) -> bool {
        self.overrides.has_scoped_package(package) || self.excludes.has_scoped_package(package)
    }

    /// Return all global override requirements that are not excluded.
    pub fn global_overrides(&self) -> impl Iterator<Item = &Requirement> {
        self.overrides
            .global_requirements()
            .filter(|requirement| !self.excludes.contains(&requirement.name))
    }

    /// Return all scoped override requirements that are not excluded in their scope.
    pub fn scoped_overrides(&self) -> impl Iterator<Item = &Requirement> {
        self.overrides
            .scoped_requirements()
            .filter(|(package, version, requirement)| {
                !self.excludes.contains_for_scope(
                    &self.overrides,
                    package,
                    *version,
                    &requirement.name,
                )
            })
            .map(|(_, _, requirement)| requirement)
    }

    /// Return the scoped override requirements that apply to this package version.
    pub fn scoped_overrides_for(
        &self,
        package: &PackageName,
        version: &Version,
    ) -> impl Iterator<Item = &Requirement> {
        self.overrides
            .scoped_requirements_for(package, version)
            .filter(|requirement| {
                !self
                    .excludes
                    .contains_for(package, version, &requirement.name)
            })
    }

    /// Return whether a dependency is globally excluded.
    pub fn is_excluded(&self, dependency: &PackageName) -> bool {
        self.excludes.contains(dependency)
    }

    /// Return whether a dependency is excluded from a specific package version.
    pub fn is_excluded_for(
        &self,
        package: &PackageName,
        version: &Version,
        dependency: &PackageName,
    ) -> bool {
        self.excludes.contains_for(package, version, dependency)
    }

    /// Apply dependency overrides and exclusions in the given scope.
    pub fn apply<'a, I>(
        &'a self,
        scope: DependencyModifierScope<'_>,
        requirements: I,
    ) -> impl Iterator<Item = Cow<'a, Requirement>> + use<'a, I>
    where
        I: IntoIterator<Item = &'a Requirement>,
    {
        let (overrides, excludes) = match scope {
            DependencyModifierScope::Global => (None, None),
            DependencyModifierScope::Package(package, version) => {
                (Some((package, version)), Some((package, version)))
            }
            DependencyModifierScope::DependencyGroup(package, version) => {
                (None, Some((package, version)))
            }
        };
        let excludes = excludes
            .and_then(|(package, version)| self.excludes.scoped_exclusions_for(package, version));
        self.overrides
            .apply_for_package(overrides, requirements)
            .filter(move |requirement| {
                !self.excludes.contains(&requirement.name)
                    && !excludes.is_some_and(|excludes| excludes.contains(&requirement.name))
            })
    }
}
