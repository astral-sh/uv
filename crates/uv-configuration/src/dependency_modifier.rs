use std::borrow::Cow;
use std::collections::BTreeSet;
use std::fmt;

use either::Either;
use rustc_hash::{FxHashMap, FxHashSet};
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

/// Dependency overrides and exclusions merged by package within each scope.
#[derive(Default, Clone, PartialEq, Eq)]
pub struct DependencyModifiers {
    global: ScopeModifiers,
    scoped: FxHashMap<PackageName, PackageModifiers>,
}

/// Temporary entries for reading and writing lockfiles and tool receipts.
#[derive(serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct DependencyModifierEntries {
    #[serde(default)]
    pub overrides: Vec<Override>,
    #[serde(default, rename = "excludes")]
    pub exclusions: Vec<ExcludeDependency>,
}

/// Display modifiers in a deterministic order, using the same entries as lockfiles.
impl fmt::Debug for DependencyModifiers {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let entries = self.to_entries();
        formatter
            .debug_struct("DependencyModifiers")
            .field("overrides", &entries.overrides)
            .field("exclusions", &entries.exclusions)
            .finish_non_exhaustive()
    }
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

impl<'de> serde::Deserialize<'de> for DependencyModifiers {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let entries = DependencyModifierEntries::deserialize(deserializer)?;
        Self::from_parts(entries.overrides, entries.exclusions).map_err(Error::custom)
    }
}

impl DependencyModifiers {
    /// Return whether the collection contains modifiers scoped to this package.
    pub fn has_scoped_package(&self, package: &PackageName) -> bool {
        self.scoped.contains_key(package)
    }

    /// Return whether the collection contains no global or scoped modifiers.
    pub fn is_empty(&self) -> bool {
        self.global.dependencies.is_empty() && self.scoped.is_empty()
    }

    /// Convert the merged modifiers to sorted entries for lockfiles and tool receipts.
    pub fn to_entries(&self) -> DependencyModifierEntries {
        let mut entries = DependencyModifierEntries {
            overrides: self
                .global
                .overrides()
                .cloned()
                .map(Override::requirement)
                .collect(),
            exclusions: self
                .global
                .dependencies
                .iter()
                .filter(|(_, modifier)| modifier.excluded)
                .map(|(name, _)| ExcludeDependency::Dependency(name.clone()))
                .collect(),
        };
        for (name, modifiers) in &self.scoped {
            modifiers.versionless.append_entries(
                PackageDependencyModifierTarget {
                    name: name.clone(),
                    version: None,
                },
                &mut entries,
            );
            for (version, scope) in &modifiers.versions {
                scope.append_entries(
                    PackageDependencyModifierTarget {
                        name: name.clone(),
                        version: Some(version.clone()),
                    },
                    &mut entries,
                );
            }
        }
        entries.overrides.sort_unstable();
        entries.overrides.dedup();
        entries.exclusions.sort_unstable();
        entries
    }

    /// Merge another collection of dependency modifiers.
    pub fn extend(&mut self, modifiers: Self) {
        self.global.extend(modifiers.global);
        for (name, package) in modifiers.scoped {
            let target = self.scoped.entry(name).or_default();
            target.versionless.extend(package.versionless);
            for (version, scope) in package.versions {
                target.versions.entry(version).or_default().extend(scope);
            }
        }
    }

    /// Merge override and exclusion inputs into modifiers keyed by package.
    pub fn from_parts(
        overrides: impl IntoIterator<Item = Override>,
        exclusions: impl IntoIterator<Item = ExcludeDependency>,
    ) -> Result<Self, ScopedOverrideSourceError> {
        let mut modifiers = Self::default();
        modifiers.extend_overrides(overrides)?;
        modifiers.extend_exclusions(exclusions);
        Ok(modifiers)
    }

    /// Add override entries to this collection.
    pub fn extend_overrides(
        &mut self,
        overrides: impl IntoIterator<Item = Override>,
    ) -> Result<(), ScopedOverrideSourceError> {
        for override_entry in overrides {
            match override_entry {
                Override::Requirement(requirement) => {
                    self.global.insert_override(*requirement);
                }
                Override::Package(package) => {
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

                    let modifiers = self.scoped.entry(package.package.name).or_default();
                    let scope = if let Some(version) = package.package.version {
                        modifiers.versions.entry(version).or_default()
                    } else {
                        &mut modifiers.versionless
                    };
                    scope.has_overrides = true;
                    for requirement in package.dependencies {
                        scope.insert_override(requirement);
                    }
                }
            }
        }
        Ok(())
    }

    /// Add exclusion entries to this collection.
    pub fn extend_exclusions(&mut self, exclusions: impl IntoIterator<Item = ExcludeDependency>) {
        for exclusion in exclusions {
            match exclusion {
                ExcludeDependency::Dependency(dependency) => {
                    self.global.insert_exclusion(dependency);
                }
                ExcludeDependency::Package(package) => {
                    let modifiers = self.scoped.entry(package.package.name).or_default();
                    let scope = if let Some(version) = package.package.version {
                        modifiers.versions.entry(version).or_default()
                    } else {
                        &mut modifiers.versionless
                    };
                    scope.has_exclusions = true;
                    for dependency in package.dependencies {
                        scope.insert_exclusion(dependency);
                    }
                }
            }
        }
    }

    /// Fallibly map override requirements, retaining exclusions and explicitly empty scopes.
    pub fn try_map_requirements<E>(
        mut self,
        mut function: impl FnMut(Requirement) -> Result<Requirement, E>,
    ) -> Result<Self, E> {
        self.global = self.global.try_map_requirements(&mut function)?;
        for package in self.scoped.values_mut() {
            package.versionless =
                std::mem::take(&mut package.versionless).try_map_requirements(&mut function)?;
            for scope in package.versions.values_mut() {
                *scope = std::mem::take(scope).try_map_requirements(&mut function)?;
            }
        }
        Ok(self)
    }

    /// Return all global override [`Requirement`]s that are not excluded.
    pub fn global_overrides(&self) -> impl Iterator<Item = &Requirement> {
        self.global
            .dependencies
            .values()
            .filter(|modifier| !modifier.excluded)
            .flat_map(|modifier| &modifier.overrides)
    }

    /// Return all scoped override [`Requirement`]s that are not excluded in their scope.
    pub fn scoped_overrides(&self) -> impl Iterator<Item = &Requirement> {
        self.scoped.iter().flat_map(move |(package, modifiers)| {
            modifiers
                .versionless
                .overrides()
                .filter(move |requirement| {
                    !self.is_excluded(&requirement.name)
                        && !modifiers.is_versionless_override_excluded(&requirement.name)
                })
                .chain(
                    modifiers
                        .versions
                        .iter()
                        .flat_map(move |(version, overrides)| {
                            overrides.overrides().filter(move |requirement| {
                                !self.is_excluded_for(package, version, &requirement.name)
                            })
                        }),
                )
        })
    }

    /// Return the scoped override [`Requirement`]s that apply to a specific package version and
    /// are not excluded.
    pub fn scoped_overrides_for(
        &self,
        package: &PackageName,
        version: &Version,
    ) -> impl Iterator<Item = &Requirement> {
        self.scoped
            .get(package)
            .and_then(|modifiers| modifiers.for_version(version).0)
            .into_iter()
            .flat_map(ScopeModifiers::overrides)
            .filter(|requirement| !self.is_excluded_for(package, version, &requirement.name))
    }

    /// Return whether a dependency is globally excluded.
    pub fn is_excluded(&self, dependency: &PackageName) -> bool {
        self.global.is_excluded(dependency)
    }

    /// Return whether a dependency is excluded from a specific package version.
    pub fn is_excluded_for(
        &self,
        package: &PackageName,
        version: &Version,
        dependency: &PackageName,
    ) -> bool {
        self.is_excluded(dependency)
            || self
                .scoped
                .get(package)
                .and_then(|modifiers| modifiers.for_version(version).1)
                .is_some_and(|scope| scope.is_excluded(dependency))
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
            DependencyModifierScope::Package(package, version) => self
                .scoped
                .get(package)
                .map(|modifiers| modifiers.for_version(version))
                .unwrap_or_default(),
            DependencyModifierScope::DependencyGroup(package, version) => {
                let exclusions = self
                    .scoped
                    .get(package)
                    .and_then(|modifiers| modifiers.for_version(version).1);
                (None, exclusions)
            }
        };
        self.apply_overrides(requirements, overrides)
            .filter(move |requirement| {
                !self.is_excluded(&requirement.name)
                    && !exclusions.is_some_and(|scope| scope.is_excluded(&requirement.name))
            })
    }
}

#[derive(Default, Clone, Eq)]
struct DependencyModifier {
    // Equal requirements can have distinct origins needed for source annotations.
    overrides: Vec<Requirement>,
    excluded: bool,
}

impl PartialEq for DependencyModifier {
    fn eq(&self, other: &Self) -> bool {
        self.excluded == other.excluded
            && self.overrides.iter().collect::<BTreeSet<_>>()
                == other.overrides.iter().collect::<BTreeSet<_>>()
    }
}

#[derive(Default, Clone, PartialEq, Eq)]
struct ScopeModifiers {
    dependencies: FxHashMap<PackageName, DependencyModifier>,
    // An explicitly empty scope replaces a less specific scope of the same kind.
    has_overrides: bool,
    has_exclusions: bool,
}

impl ScopeModifiers {
    fn insert_override(&mut self, requirement: Requirement) {
        self.has_overrides = true;
        self.dependencies
            .entry(requirement.name.clone())
            .or_default()
            .overrides
            .push(requirement);
    }

    fn insert_exclusion(&mut self, name: PackageName) {
        self.has_exclusions = true;
        self.dependencies.entry(name).or_default().excluded = true;
    }

    fn overrides(&self) -> impl Iterator<Item = &Requirement> {
        self.dependencies
            .values()
            .flat_map(|modifier| &modifier.overrides)
    }

    fn is_excluded(&self, dependency: &PackageName) -> bool {
        self.dependencies
            .get(dependency)
            .is_some_and(|modifier| modifier.excluded)
    }

    fn extend(&mut self, scope: Self) {
        self.has_overrides |= scope.has_overrides;
        self.has_exclusions |= scope.has_exclusions;
        for (name, modifier) in scope.dependencies {
            let target = self.dependencies.entry(name).or_default();
            target.overrides.extend(modifier.overrides);
            target.excluded |= modifier.excluded;
        }
    }

    fn try_map_requirements<E>(
        self,
        function: &mut impl FnMut(Requirement) -> Result<Requirement, E>,
    ) -> Result<Self, E> {
        let mut mapped = Self {
            has_overrides: self.has_overrides,
            has_exclusions: self.has_exclusions,
            ..Self::default()
        };
        for (name, modifier) in self.dependencies {
            if modifier.excluded {
                mapped.insert_exclusion(name);
            }
            for requirement in modifier.overrides {
                mapped.insert_override(function(requirement)?);
            }
        }
        Ok(mapped)
    }

    fn append_entries(
        &self,
        package: PackageDependencyModifierTarget,
        entries: &mut DependencyModifierEntries,
    ) {
        if self.has_overrides {
            let mut dependencies = self.overrides().cloned().collect::<Vec<_>>();
            dependencies.sort_unstable();
            dependencies.dedup();
            entries.overrides.push(Override::Package(PackageOverride {
                package: package.clone(),
                dependencies: dependencies.into_boxed_slice(),
            }));
        }
        if self.has_exclusions {
            let mut dependencies = self
                .dependencies
                .iter()
                .filter(|(_, modifier)| modifier.excluded)
                .map(|(name, _)| name.clone())
                .collect::<Vec<_>>();
            dependencies.sort_unstable();
            entries
                .exclusions
                .push(ExcludeDependency::Package(PackageExclusion {
                    package,
                    dependencies: dependencies.into_boxed_slice(),
                }));
        }
    }
}

#[derive(Default, Clone, PartialEq, Eq)]
struct PackageModifiers {
    versionless: ScopeModifiers,
    versions: FxHashMap<Version, ScopeModifiers>,
}

impl PackageModifiers {
    /// Select the most specific override and exclusion scopes independently. An exact-version
    /// scope replaces the versionless scope, including when it is empty.
    fn for_version(&self, version: &Version) -> (Option<&ScopeModifiers>, Option<&ScopeModifiers>) {
        let exact = self.versions.get(version);
        (
            exact
                .filter(|scope| scope.has_overrides)
                .or(self.versionless.has_overrides.then_some(&self.versionless)),
            exact
                .filter(|scope| scope.has_exclusions)
                .or(self.versionless.has_exclusions.then_some(&self.versionless)),
        )
    }

    /// Return whether a dependency is excluded everywhere its versionless override applies.
    /// Exact-version exclusions can allow it again, unless an exact-version override shadows it.
    fn is_versionless_override_excluded(&self, dependency: &PackageName) -> bool {
        self.versionless.is_excluded(dependency)
            && self
                .versions
                .values()
                .filter(|scope| scope.has_exclusions && !scope.has_overrides)
                .all(|scope| scope.is_excluded(dependency))
    }
}

impl DependencyModifiers {
    fn apply_overrides<'a, I>(
        &'a self,
        requirements: I,
        scoped: Option<&'a ScopeModifiers>,
    ) -> impl Iterator<Item = Cow<'a, Requirement>> + use<'a, I>
    where
        I: IntoIterator<Item = &'a Requirement>,
    {
        if let Some(scoped) = scoped {
            let requirements = requirements.into_iter().collect::<Vec<_>>();
            let names = requirements
                .iter()
                .map(|requirement| requirement.name.clone())
                .collect::<FxHashSet<_>>();
            let mut additions = scoped
                .overrides()
                .filter(|requirement| !names.contains(&requirement.name))
                .collect::<Vec<_>>();
            additions.sort_unstable();

            Either::Left(
                requirements
                    .into_iter()
                    .flat_map(move |requirement| self.apply_requirement(requirement, Some(scoped)))
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
        scoped: Option<&'a ScopeModifiers>,
    ) -> impl Iterator<Item = Cow<'a, Requirement>> {
        let Some(overrides) = scoped
            .and_then(|scope| scope.dependencies.get(&requirement.name))
            .filter(|modifier| !modifier.overrides.is_empty())
            .or_else(|| self.global.dependencies.get(&requirement.name))
            .filter(|modifier| !modifier.overrides.is_empty())
            .map(|modifier| &modifier.overrides)
        else {
            return Either::Left(std::iter::once(Cow::Borrowed(requirement)));
        };

        // ASSUMPTION: There is one `extra = "..."`, and it's either the only marker or part
        // of the main conjunction.
        let Some(extra_expression) = requirement.marker.top_level_extra() else {
            return Either::Right(Either::Left(overrides.iter().map(Cow::Borrowed)));
        };

        // When the original requirement is an optional dependency, the override(s) need to
        // be optional for the same extra, otherwise we activate extras that should be inactive.
        Either::Right(Either::Right(overrides.iter().map(
            move |override_requirement| {
                let marker = MarkerTree::expression(extra_expression.clone())
                    .and(override_requirement.marker);
                Cow::Owned(Requirement {
                    marker,
                    ..override_requirement.clone()
                })
            },
        )))
    }
}
