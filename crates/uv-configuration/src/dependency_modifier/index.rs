use std::borrow::Cow;

use either::Either;
use rustc_hash::{FxHashMap, FxHashSet};

use uv_distribution_types::{Requirement, RequirementSource};
use uv_normalize::PackageName;
use uv_pep440::Version;
use uv_pep508::MarkerTree;

use super::{
    DependencyExclusion, DependencyModifierScope, DependencyModifiers, DependencyOverride,
    ScopedOverrideSourceError,
};

type OverrideMap = FxHashMap<PackageName, Vec<Requirement>>;
type ExclusionSet = FxHashSet<PackageName>;

#[derive(Default, Clone, PartialEq, Eq)]
pub(super) struct DependencyModifierIndex {
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
    pub(super) fn insert_override(
        &mut self,
        override_entry: &DependencyOverride,
    ) -> Result<(), ScopedOverrideSourceError> {
        match override_entry {
            DependencyOverride::Requirement(requirement) => {
                self.global_overrides
                    .entry(requirement.name.clone())
                    .or_default()
                    .push(requirement.as_ref().clone());
            }
            DependencyOverride::Package(package) => {
                for requirement in &package.dependencies {
                    match &requirement.source {
                        RequirementSource::Registry { index: None, .. } => {}
                        RequirementSource::Registry { index: Some(_), .. } => {
                            return Err(ScopedOverrideSourceError::Index {
                                package: package.package.name.clone(),
                                dependency: requirement.name.clone(),
                            });
                        }
                        _ => {
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

    pub(super) fn insert_exclusion(&mut self, exclusion: &DependencyExclusion) {
        match exclusion {
            DependencyExclusion::Dependency(dependency) => {
                self.global_exclusions.insert(dependency.clone());
            }
            DependencyExclusion::Package(package) => {
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

        let Some(extra_expression) = requirement.marker.top_level_extra() else {
            return Either::Right(Either::Left(overrides.iter().map(Cow::Borrowed)));
        };

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

impl DependencyModifiers {
    /// Return all global override [`Requirement`]s that are not excluded.
    pub fn global_overrides(&self) -> impl Iterator<Item = &Requirement> {
        self.index
            .global_overrides
            .values()
            .flatten()
            .filter(|requirement| !self.is_excluded(&requirement.name))
    }

    /// Return all scoped override [`Requirement`]s that are not excluded in their scope.
    pub fn scoped_overrides(
        &self,
    ) -> impl Iterator<Item = (&PackageName, Option<&Version>, &Requirement)> {
        self.index
            .scoped
            .iter()
            .flat_map(move |(package, modifiers)| {
                modifiers
                    .overrides
                    .iter()
                    .flat_map(|overrides| overrides.values().flatten())
                    .filter_map(move |requirement| {
                        (!self.is_excluded(&requirement.name)
                            && !modifiers.is_versionless_override_excluded(&requirement.name))
                        .then_some((package, None, requirement))
                    })
                    .chain(modifiers.override_versions.iter().flat_map(
                        move |(version, overrides)| {
                            overrides.values().flatten().filter_map(move |requirement| {
                                (!self.is_excluded_for(package, version, &requirement.name))
                                    .then_some((package, Some(version), requirement))
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
