use std::borrow::Cow;
use std::fmt;

use either::Either;
use rustc_hash::{FxHashMap, FxHashSet};
use serde::de::Error;

use uv_distribution_types::{Requirement, RequirementSource};
use uv_normalize::PackageName;
use uv_pep440::Version;
use uv_pep508::MarkerTree;

use crate::{ExcludeDependency, Override, ScopedOverrideSourceError};

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

/// An indexed collection of dependency overrides and exclusions.
#[derive(Default, Clone, PartialEq, Eq)]
pub struct DependencyModifiers {
    entries: DependencyModifierEntries,
    index: DependencyModifierIndex,
}

#[derive(Default, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
struct DependencyModifierEntries {
    #[serde(default)]
    overrides: Vec<Override>,
    #[serde(default, rename = "excludes")]
    exclusions: Vec<ExcludeDependency>,
}

/// Custom `Debug` to hide the derived index from `--show-settings` output.
impl fmt::Debug for DependencyModifiers {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DependencyModifiers")
            .field("overrides", &self.entries.overrides)
            .field("exclusions", &self.entries.exclusions)
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
    /// Return the override entries.
    pub fn override_entries(&self) -> impl Iterator<Item = &Override> {
        self.entries.overrides.iter()
    }

    /// Return the exclusion entries.
    pub fn exclusion_entries(&self) -> impl Iterator<Item = &ExcludeDependency> {
        self.entries.exclusions.iter()
    }

    /// Consume the collection and return its override and exclusion entries.
    pub fn into_parts(self) -> (Vec<Override>, Vec<ExcludeDependency>) {
        (self.entries.overrides, self.entries.exclusions)
    }

    /// Add all entries from another collection of dependency modifiers.
    pub fn extend(&mut self, modifiers: Self) -> Result<(), ScopedOverrideSourceError> {
        let (overrides, exclusions) = modifiers.into_parts();
        self.extend_overrides(overrides)?;
        self.extend_exclusions(exclusions);
        Ok(())
    }

    /// Create an indexed collection from separate override and exclusion wire entries.
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
        for entry in overrides {
            self.index.insert_override(&entry)?;
            self.entries.overrides.push(entry);
        }
        Ok(())
    }

    /// Add exclusion entries to this collection.
    pub fn extend_exclusions(&mut self, exclusions: impl IntoIterator<Item = ExcludeDependency>) {
        for entry in exclusions {
            self.index.insert_exclusion(&entry);
            self.entries.exclusions.push(entry);
        }
    }

    /// Return all global override [`Requirement`]s that are not excluded.
    pub fn global_overrides(&self) -> impl Iterator<Item = &Requirement> {
        self.index
            .global_overrides
            .values()
            .flatten()
            .filter(|requirement| !self.is_excluded(&requirement.name))
    }

    /// Return all scoped override [`Requirement`]s that are not excluded in their scope.
    pub fn scoped_overrides(&self) -> impl Iterator<Item = &Requirement> {
        self.index
            .scoped
            .iter()
            .flat_map(move |(package, modifiers)| {
                modifiers
                    .overrides
                    .iter()
                    .flat_map(|overrides| overrides.values().flatten())
                    .filter(move |requirement| {
                        !self.is_excluded(&requirement.name)
                            && !modifiers.is_versionless_override_excluded(&requirement.name)
                    })
                    .chain(modifiers.override_versions.iter().flat_map(
                        move |(version, overrides)| {
                            overrides.values().flatten().filter(move |requirement| {
                                !self.is_excluded_for(package, version, &requirement.name)
                            })
                        },
                    ))
            })
    }

    /// Return the scoped override [`Requirement`]s that apply to a specific package version and
    /// are not excluded.
    pub fn scoped_overrides_for(
        &self,
        package: &PackageName,
        version: &Version,
    ) -> impl Iterator<Item = &Requirement> {
        self.index
            .scoped
            .get(package)
            .and_then(|modifiers| modifiers.for_version(version).0)
            .into_iter()
            .flat_map(|overrides| overrides.values().flatten())
            .filter(|requirement| !self.is_excluded_for(package, version, &requirement.name))
    }

    /// Return whether a dependency is globally excluded.
    pub fn is_excluded(&self, dependency: &PackageName) -> bool {
        self.index.is_excluded(None, dependency)
    }

    /// Return whether a dependency is excluded from a specific package version.
    pub fn is_excluded_for(
        &self,
        package: &PackageName,
        version: &Version,
        dependency: &PackageName,
    ) -> bool {
        self.index.is_excluded(
            self.index
                .scoped
                .get(package)
                .and_then(|modifiers| modifiers.for_version(version).1),
            dependency,
        )
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
                .index
                .scoped
                .get(package)
                .map(|modifiers| modifiers.for_version(version))
                .unwrap_or_default(),
            DependencyModifierScope::DependencyGroup(package, version) => {
                let exclusions = self
                    .index
                    .scoped
                    .get(package)
                    .and_then(|modifiers| modifiers.for_version(version).1);
                (None, exclusions)
            }
        };
        self.index
            .apply_overrides(requirements, overrides)
            .filter(move |requirement| !self.index.is_excluded(exclusions, &requirement.name))
    }
}

type OverrideMap = FxHashMap<PackageName, Vec<Requirement>>;
type ExclusionSet = FxHashSet<PackageName>;

#[derive(Default, Clone, PartialEq, Eq)]
struct DependencyModifierIndex {
    global_overrides: OverrideMap,
    global_exclusions: ExclusionSet,
    scoped: FxHashMap<PackageName, PackageModifiers>,
}

#[derive(Default, Clone, PartialEq, Eq)]
struct PackageModifiers {
    overrides: Option<OverrideMap>,
    exclusions: Option<ExclusionSet>,
    override_versions: FxHashMap<Version, OverrideMap>,
    exclusion_versions: FxHashMap<Version, ExclusionSet>,
}

impl PackageModifiers {
    fn for_version(&self, version: &Version) -> (Option<&OverrideMap>, Option<&ExclusionSet>) {
        (
            self.override_versions
                .get(version)
                .or(self.overrides.as_ref()),
            self.exclusion_versions
                .get(version)
                .or(self.exclusions.as_ref()),
        )
    }

    fn is_versionless_override_excluded(&self, dependency: &PackageName) -> bool {
        self.exclusions
            .as_ref()
            .is_some_and(|exclusions| exclusions.contains(dependency))
            && self
                .exclusion_versions
                .iter()
                .filter(|(version, _)| !self.override_versions.contains_key(*version))
                .all(|(_, exclusions)| exclusions.contains(dependency))
    }
}

impl DependencyModifierIndex {
    fn insert_override(
        &mut self,
        override_entry: &Override,
    ) -> Result<(), ScopedOverrideSourceError> {
        match override_entry {
            Override::Requirement(requirement) => {
                self.global_overrides
                    .entry(requirement.name.clone())
                    .or_default()
                    .push(requirement.as_ref().clone());
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

                let modifiers = self.scoped.entry(package.package.name.clone()).or_default();
                let overrides = if let Some(version) = package.package.version.clone() {
                    modifiers.override_versions.entry(version).or_default()
                } else {
                    modifiers.overrides.get_or_insert_default()
                };
                for requirement in &package.dependencies {
                    overrides
                        .entry(requirement.name.clone())
                        .or_default()
                        .push(requirement.clone());
                }
            }
        }
        Ok(())
    }

    fn insert_exclusion(&mut self, exclusion: &ExcludeDependency) {
        match exclusion {
            ExcludeDependency::Dependency(dependency) => {
                self.global_exclusions.insert(dependency.clone());
            }
            ExcludeDependency::Package(package) => {
                let modifiers = self.scoped.entry(package.package.name.clone()).or_default();
                let exclusions = if let Some(version) = package.package.version.clone() {
                    modifiers.exclusion_versions.entry(version).or_default()
                } else {
                    modifiers.exclusions.get_or_insert_default()
                };
                exclusions.extend(package.dependencies.iter().cloned());
            }
        }
    }

    fn is_excluded(&self, scoped: Option<&ExclusionSet>, dependency: &PackageName) -> bool {
        self.global_exclusions.contains(dependency)
            || scoped.is_some_and(|exclusions| exclusions.contains(dependency))
    }

    fn apply_overrides<'a, I>(
        &'a self,
        requirements: I,
        scoped: Option<&'a OverrideMap>,
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
                .iter()
                .filter(|(name, _)| !names.contains(*name))
                .flat_map(|(_, requirements)| requirements)
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
        scoped: Option<&'a OverrideMap>,
    ) -> impl Iterator<Item = Cow<'a, Requirement>> {
        let Some(overrides) = scoped
            .and_then(|overrides| overrides.get(&requirement.name))
            .or_else(|| self.global_overrides.get(&requirement.name))
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
