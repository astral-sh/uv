use std::borrow::Cow;

use either::Either;
use serde::Deserialize;
use serde::de::Error;

use uv_distribution_types::{Requirement, RequirementSource};
use uv_normalize::PackageName;
use uv_pep440::Version;
use uv_pep508::MarkerTree;

use crate::{
    ExcludeDependency, Override, PackageExclusion, PackageOverride, ScopedOverrideSourceError,
};

/// The package and optional version selected by a dependency modifier.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct PackageDependencyModifierTarget {
    name: PackageName,
    #[cfg_attr(
        feature = "schemars",
        schemars(
            with = "Option<String>",
            description = "PEP 440-style package version, e.g., `1.2.3`"
        )
    )]
    version: Option<Version>,
}

/// Dependency override and exclusion inputs.
#[derive(Debug, Default, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct DependencyModifiers {
    #[serde(default, deserialize_with = "deserialize_overrides")]
    pub overrides: Vec<Override>,
    #[serde(default)]
    pub excludes: Vec<ExcludeDependency>,
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
    /// Collect override and exclusion inputs.
    pub fn from_parts(
        overrides: impl IntoIterator<Item = Override>,
        exclusions: impl IntoIterator<Item = ExcludeDependency>,
    ) -> Result<Self, ScopedOverrideSourceError> {
        let overrides = overrides.into_iter().collect::<Vec<_>>();
        validate_overrides(&overrides)?;
        Ok(Self {
            overrides,
            excludes: exclusions.into_iter().collect(),
        })
    }

    /// Append another set of dependency modifier inputs.
    pub fn extend(&mut self, modifiers: Self) {
        self.overrides.extend(modifiers.overrides);
        self.excludes.extend(modifiers.excludes);
    }

    /// Return whether the collection is empty.
    pub fn is_empty(&self) -> bool {
        self.overrides.is_empty() && self.excludes.is_empty()
    }

    /// Normalize override requirements for lockfile comparison and serialization.
    pub fn try_normalize_requirements<E>(
        mut self,
        mut function: impl FnMut(Requirement) -> Result<Requirement, E>,
    ) -> Result<Self, E> {
        self.overrides = self
            .overrides
            .into_iter()
            .map(|entry| {
                Ok(match entry {
                    Override::Requirement(requirement) => {
                        Override::requirement(function(*requirement)?)
                    }
                    Override::Package(package) => Override::Package(PackageOverride {
                        package: package.package,
                        dependencies: package
                            .dependencies
                            .into_vec()
                            .into_iter()
                            .map(&mut function)
                            .collect::<Result<_, E>>()?,
                    }),
                })
            })
            .collect::<Result<_, E>>()?;
        self.overrides.sort_unstable();
        self.overrides.dedup();
        self.excludes.sort_unstable();
        self.excludes.dedup();
        Ok(self)
    }

    /// Return whether the collection contains modifiers scoped to this package.
    pub fn has_scoped_package(&self, package: &PackageName) -> bool {
        self.package_overrides()
            .map(|entry| &entry.package)
            .chain(self.package_exclusions().map(|entry| &entry.package))
            .any(|target| &target.name == package)
    }

    /// Return all global override requirements that are not excluded.
    pub fn global_overrides(&self) -> impl Iterator<Item = &Requirement> {
        self.overrides
            .iter()
            .filter_map(|entry| match entry {
                Override::Requirement(requirement) => Some(requirement.as_ref()),
                Override::Package(_) => None,
            })
            .filter(|requirement| !self.is_excluded(&requirement.name))
    }

    /// Return all scoped override requirements that are not excluded in their scope.
    pub fn scoped_overrides(&self) -> impl Iterator<Item = &Requirement> {
        self.package_overrides().flat_map(move |entry| {
            entry.dependencies.iter().filter(move |requirement| {
                if let Some(version) = &entry.package.version {
                    !self.is_excluded_for(&entry.package.name, version, &requirement.name)
                } else {
                    !self.is_excluded(&requirement.name)
                        && !self.is_versionless_override_excluded(&entry.package, &requirement.name)
                }
            })
        })
    }

    /// Return scoped override requirements that apply to this package version and are not excluded.
    pub fn scoped_overrides_for(
        &self,
        package: &PackageName,
        version: &Version,
    ) -> impl Iterator<Item = &Requirement> {
        let overrides = find_scope(
            self.package_overrides().map(|entry| &entry.package),
            package,
            version,
        );
        let exclusions = find_scope(
            self.package_exclusions().map(|entry| &entry.package),
            package,
            version,
        );
        self.overrides_in(overrides).filter(move |requirement| {
            !self.is_excluded(&requirement.name)
                && !self.is_excluded_in(exclusions, &requirement.name)
        })
    }

    /// Return whether a dependency is globally excluded.
    pub fn is_excluded(&self, dependency: &PackageName) -> bool {
        self.excludes.iter().any(|entry| match entry {
            ExcludeDependency::Dependency(name) => name == dependency,
            ExcludeDependency::Package(_) => false,
        })
    }

    /// Return whether a dependency is excluded from a specific package version.
    pub fn is_excluded_for(
        &self,
        package: &PackageName,
        version: &Version,
        dependency: &PackageName,
    ) -> bool {
        let scope = find_scope(
            self.package_exclusions().map(|entry| &entry.package),
            package,
            version,
        );
        self.is_excluded(dependency) || self.is_excluded_in(scope, dependency)
    }

    /// Apply dependency modifiers in a specific [`DependencyModifierScope`].
    ///
    /// NB: Change this method together with [`Constraints::apply`](crate::Constraints::apply).
    pub fn apply<'a, I>(
        &'a self,
        scope: DependencyModifierScope<'_>,
        requirements: I,
    ) -> impl Iterator<Item = Cow<'a, Requirement>> + use<'a, I>
    where
        I: IntoIterator<Item = &'a Requirement>,
    {
        let (overrides, exclusions) = match scope {
            DependencyModifierScope::Global => (None, None),
            DependencyModifierScope::Package(package, version) => (
                find_scope(
                    self.package_overrides().map(|entry| &entry.package),
                    package,
                    version,
                ),
                find_scope(
                    self.package_exclusions().map(|entry| &entry.package),
                    package,
                    version,
                ),
            ),
            DependencyModifierScope::DependencyGroup(package, version) => (
                None,
                find_scope(
                    self.package_exclusions().map(|entry| &entry.package),
                    package,
                    version,
                ),
            ),
        };
        self.apply_overrides(requirements, overrides)
            .filter(move |requirement| {
                !self.is_excluded(&requirement.name)
                    && !self.is_excluded_in(exclusions, &requirement.name)
            })
    }

    fn package_overrides(&self) -> impl Iterator<Item = &PackageOverride> + Clone {
        self.overrides.iter().filter_map(|entry| match entry {
            Override::Package(package) => Some(package),
            Override::Requirement(_) => None,
        })
    }

    fn package_exclusions(&self) -> impl Iterator<Item = &PackageExclusion> + Clone {
        self.excludes.iter().filter_map(|entry| match entry {
            ExcludeDependency::Package(package) => Some(package),
            ExcludeDependency::Dependency(_) => None,
        })
    }

    fn overrides_in<'a>(
        &'a self,
        scope: Option<&'a PackageDependencyModifierTarget>,
    ) -> impl Iterator<Item = &'a Requirement> {
        self.package_overrides()
            .filter(move |entry| Some(&entry.package) == scope)
            .flat_map(|entry| entry.dependencies.iter())
    }

    fn is_excluded_in(
        &self,
        scope: Option<&PackageDependencyModifierTarget>,
        dependency: &PackageName,
    ) -> bool {
        self.package_exclusions()
            .any(|entry| Some(&entry.package) == scope && entry.dependencies.contains(dependency))
    }

    /// Exact exclusions may re-enable a dependency where its versionless override still applies.
    fn is_versionless_override_excluded(
        &self,
        scope: &PackageDependencyModifierTarget,
        dependency: &PackageName,
    ) -> bool {
        self.is_excluded_in(Some(scope), dependency)
            && self
                .package_exclusions()
                .filter(|entry| {
                    entry.package.name == scope.name
                        && entry.package.version.is_some()
                        && !self
                            .package_overrides()
                            .any(|overrides| overrides.package == entry.package)
                })
                .all(|entry| self.is_excluded_in(Some(&entry.package), dependency))
    }

    fn apply_overrides<'a, I>(
        &'a self,
        requirements: I,
        scoped: Option<&'a PackageDependencyModifierTarget>,
    ) -> impl Iterator<Item = Cow<'a, Requirement>> + use<'a, I>
    where
        I: IntoIterator<Item = &'a Requirement>,
    {
        if scoped.is_some() {
            let requirements = requirements.into_iter().collect::<Vec<_>>();
            let mut additions = self
                .overrides_in(scoped)
                .filter(|addition| {
                    !requirements
                        .iter()
                        .any(|requirement| requirement.name == addition.name)
                })
                .collect::<Vec<_>>();
            additions.sort_unstable();
            Either::Left(
                requirements
                    .into_iter()
                    .flat_map(move |requirement| self.apply_requirement(requirement, scoped))
                    .chain(additions.into_iter().map(Cow::Borrowed)),
            )
        } else {
            Either::Right(
                requirements
                    .into_iter()
                    .flat_map(|requirement| self.apply_requirement(requirement, None)),
            )
        }
    }

    fn apply_requirement<'a>(
        &'a self,
        requirement: &'a Requirement,
        scoped: Option<&'a PackageDependencyModifierTarget>,
    ) -> impl Iterator<Item = Cow<'a, Requirement>> {
        let mut scoped = self
            .overrides_in(scoped)
            .filter(|entry| entry.name == requirement.name)
            .peekable();
        let overrides = if scoped.peek().is_some() {
            Either::Left(scoped)
        } else {
            Either::Right(
                self.global_overrides()
                    .filter(|entry| entry.name == requirement.name),
            )
        };
        let mut overrides = overrides.peekable();
        if overrides.peek().is_none() {
            return Either::Left(std::iter::once(Cow::Borrowed(requirement)));
        }
        // An override of an optional dependency must remain optional for the same extra.
        let extra_expression = requirement.marker.top_level_extra();
        Either::Right(overrides.map(move |requirement| {
            if let Some(extra_expression) = &extra_expression {
                Cow::Owned(Requirement {
                    marker: MarkerTree::expression(extra_expression.clone())
                        .and(requirement.marker),
                    ..requirement.clone()
                })
            } else {
                Cow::Borrowed(requirement)
            }
        }))
    }
}

/// Exact-version entries replace versionless entries, even when their dependency list is empty.
fn find_scope<'a>(
    mut scopes: impl Iterator<Item = &'a PackageDependencyModifierTarget> + Clone,
    package: &PackageName,
    version: &Version,
) -> Option<&'a PackageDependencyModifierTarget> {
    scopes
        .clone()
        .find(|scope| &scope.name == package && scope.version.as_ref() == Some(version))
        .or_else(|| scopes.find(|scope| &scope.name == package && scope.version.is_none()))
}

fn validate_overrides(overrides: &[Override]) -> Result<(), ScopedOverrideSourceError> {
    for entry in overrides {
        if let Override::Package(package) = entry {
            for requirement in &package.dependencies {
                match &requirement.source {
                    RequirementSource::Registry { index: None, .. } => {}
                    RequirementSource::Registry { index: Some(_), .. } => {
                        return Err(ScopedOverrideSourceError::Index {
                            package: package.package.name.clone(),
                            dependency: requirement.name.clone(),
                        });
                    }
                    RequirementSource::Url { .. }
                    | RequirementSource::GitDirectory { .. }
                    | RequirementSource::GitPath { .. }
                    | RequirementSource::Path { .. }
                    | RequirementSource::Directory { .. } => {
                        return Err(ScopedOverrideSourceError::Url {
                            package: package.package.name.clone(),
                            dependency: requirement.name.clone(),
                        });
                    }
                }
            }
        }
    }
    Ok(())
}

fn deserialize_overrides<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<Override>, D::Error> {
    let overrides = Vec::deserialize(deserializer)?;
    validate_overrides(&overrides).map_err(Error::custom)?;
    Ok(overrides)
}
