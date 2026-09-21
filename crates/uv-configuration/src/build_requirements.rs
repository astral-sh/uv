use std::borrow::Cow;

use uv_distribution_types::{NameRequirementSpecification, Requirement};
use uv_normalize::PackageName;
use uv_pep440::Version;

use crate::{Constraint, Constraints, ExcludeDependency, Excludes, Override, Overrides};

/// Dependency policies selected by the identity of the package whose build environment is resolved.
#[derive(Debug, Default, Clone)]
pub struct BuildRequirements {
    entries: Vec<Constraint<NameRequirementSpecification>>,
    global: Constraints,
    overrides: Vec<Override<Requirement>>,
    excludes: Vec<ExcludeDependency>,
}

impl BuildRequirements {
    /// Create global build constraints, preserving their archive hashes.
    pub fn from_specifications(
        specifications: impl IntoIterator<Item = NameRequirementSpecification>,
    ) -> Self {
        Self::from_entries(specifications.into_iter().map(Constraint::Requirement))
    }

    /// Retain global and package-scoped declarations for individual build environments.
    pub fn from_entries(
        entries: impl IntoIterator<Item = Constraint<NameRequirementSpecification>>,
    ) -> Self {
        let entries: Vec<_> = entries.into_iter().collect();
        let global = Constraints::from_specifications(
            entries
                .iter()
                .filter_map(Constraint::as_requirement)
                .cloned(),
        );
        Self {
            entries,
            global,
            overrides: Vec::new(),
            excludes: Vec::new(),
        }
    }

    /// Add constraints supplied outside the workspace configuration.
    #[must_use]
    pub fn with_constraints(
        self,
        constraints: impl IntoIterator<Item = NameRequirementSpecification>,
    ) -> Self {
        Self::from_entries(
            constraints
                .into_iter()
                .map(Constraint::Requirement)
                .chain(self.entries),
        )
        .with_overrides(self.overrides)
        .with_excludes(self.excludes)
    }

    /// Configure overrides without applying them to the runtime dependency graph.
    #[must_use]
    pub fn with_overrides(
        mut self,
        overrides: impl IntoIterator<Item = Override<Requirement>>,
    ) -> Self {
        self.overrides = overrides.into_iter().collect();
        self
    }

    /// Return the override declarations recorded in the lockfile.
    pub fn override_entries(&self) -> impl Iterator<Item = &Override<Requirement>> {
        self.overrides.iter()
    }

    /// Select overrides for an entire build environment.
    ///
    /// Exact-version scopes replace name-only scopes. The selected scope replaces global overrides
    /// for the dependency names it contains; unmentioned global overrides still apply.
    pub fn overrides_for_package(
        &self,
        name: Option<&PackageName>,
        version: Option<&Version>,
    ) -> Overrides {
        let exact = self.overrides.iter().any(|entry| match entry {
            Override::Requirement(_) => false,
            Override::Package(package) => {
                package.package.version().is_some()
                    && name.is_some_and(|name| package.package.matches(name, version))
            }
        });
        let scoped: Vec<_> = self
            .overrides
            .iter()
            .flat_map(|entry| match entry {
                Override::Package(package)
                    if package.package.version().is_some() == exact
                        && name.is_some_and(|name| package.package.matches(name, version)) =>
                {
                    package.dependencies.as_ref()
                }
                Override::Package(_) | Override::Requirement(_) => &[],
            })
            .collect();
        Overrides::from_requirements(
            self.overrides
                .iter()
                .filter_map(Override::as_requirement)
                .filter(|requirement| !scoped.iter().any(|scoped| scoped.name == requirement.name))
                .chain(scoped.iter().copied())
                .cloned()
                .collect(),
        )
    }

    /// Configure exclusions for build environments.
    #[must_use]
    pub fn with_excludes(mut self, excludes: impl IntoIterator<Item = ExcludeDependency>) -> Self {
        self.excludes = excludes.into_iter().collect();
        self
    }

    /// Return the exclusion declarations recorded in the lockfile.
    pub fn exclude_entries(&self) -> impl Iterator<Item = &ExcludeDependency> {
        self.excludes.iter()
    }

    /// Select global exclusions and the most specific matching scope for a build environment.
    pub fn excludes_for_package(
        &self,
        name: Option<&PackageName>,
        version: Option<&Version>,
    ) -> Excludes {
        let exact = self.excludes.iter().any(|entry| match entry {
            ExcludeDependency::Dependency(_) => false,
            ExcludeDependency::Package(package) => {
                Some(&package.package.name) == name
                    && package.package.version.is_some()
                    && package.package.version.as_ref() == version
            }
        });
        self.excludes
            .iter()
            .flat_map(|entry| match entry {
                ExcludeDependency::Dependency(dependency) => std::slice::from_ref(dependency),
                ExcludeDependency::Package(package)
                    if Some(&package.package.name) == name
                        && package.package.version.is_some() == exact
                        && (package.package.version.is_none()
                            || package.package.version.as_ref() == version) =>
                {
                    package.dependencies.as_ref()
                }
                ExcludeDependency::Package(_) => &[],
            })
            .cloned()
            .collect()
    }

    /// Return the constraint declarations, including scopes and archive hashes, in input order.
    pub fn entries(&self) -> impl Iterator<Item = &Constraint<NameRequirementSpecification>> {
        self.entries.iter()
    }

    /// Return the constraints shared by every build environment.
    pub fn global(&self) -> &Constraints {
        &self.global
    }

    /// Whether this build environment has package-specific dependency settings.
    pub fn has_scope(&self, name: Option<&PackageName>, version: Option<&Version>) -> bool {
        self.excludes.iter().any(|entry| match entry {
            ExcludeDependency::Dependency(_) => false,
            ExcludeDependency::Package(package) => {
                Some(&package.package.name) == name
                    && (package.package.version.is_none()
                        || package.package.version.as_ref() == version)
            }
        }) || self.overrides.iter().any(|entry| match entry {
            Override::Requirement(_) => false,
            Override::Package(package) => {
                name.is_some_and(|name| package.package.matches(name, version))
            }
        }) || self.entries.iter().any(|entry| match entry {
            Constraint::Requirement(_) => false,
            Constraint::Package(package) => {
                name.is_some_and(|name| package.package.matches(name, version))
            }
        })
    }

    /// Select constraints for an entire build dependency graph.
    ///
    /// An exact-version scope is inactive when the package's version is not yet known.
    pub fn for_package(
        &self,
        name: Option<&PackageName>,
        version: Option<&Version>,
    ) -> Cow<'_, Constraints> {
        if !self.has_scope(name, version) {
            return Cow::Borrowed(&self.global);
        }
        Cow::Owned(Constraints::from_specifications(
            self.entries
                .iter()
                .flat_map(|entry| match entry {
                    Constraint::Requirement(requirement) => std::slice::from_ref(requirement),
                    Constraint::Package(package)
                        if name.is_some_and(|name| package.package.matches(name, version)) =>
                    {
                        &package.dependencies
                    }
                    Constraint::Package(_) => &[],
                })
                .cloned(),
        ))
    }
}
