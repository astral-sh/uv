use std::borrow::Cow;

use either::Either;
use rustc_hash::{FxHashMap, FxHashSet};
use serde::de::IntoDeserializer;

use uv_distribution_types::{Requirement, RequirementSource};
use uv_normalize::PackageName;
use uv_pep440::Version;
use uv_pep508::MarkerTree;

/// A dependency modifier that applies to the dependencies of a specific package version.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[cfg_attr(
    feature = "schemars",
    derive(schemars::JsonSchema),
    schemars(rename = "PackageDependencyModifier_for_{T}")
)]
#[serde(
    rename_all = "kebab-case",
    deny_unknown_fields,
    bound(
        serialize = "T: serde::Serialize",
        deserialize = "T: serde::Deserialize<'de>"
    )
)]
pub struct PackageDependencyModifier<T> {
    pub package: PackageDependencyModifierTarget,
    pub dependencies: Box<[T]>,
}

/// The package and optional version selected by a [`PackageDependencyModifier`].
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct PackageDependencyModifierTarget {
    pub(crate) name: PackageName,
    #[cfg_attr(
        feature = "schemars",
        schemars(
            with = "Option<String>",
            description = "PEP 440-style package version, e.g., `1.2.3`"
        )
    )]
    pub(crate) version: Option<Version>,
}

/// A dependency modifier, either global or scoped to a specific package version.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[cfg_attr(
    feature = "schemars",
    derive(schemars::JsonSchema),
    schemars(untagged, rename = "DependencyModifier_for_{T}")
)]
#[serde(untagged, bound(serialize = "T: serde::Serialize"))]
pub enum DependencyModifier<T> {
    Package(PackageDependencyModifier<T>),
    Dependency(T),
}

// A derived `#[serde(untagged)]` implementation collapses detailed dependency parse errors into
// "data did not match any variant", so use a type-directed visitor for string dependencies.
impl<'de, T> serde::Deserialize<'de> for DependencyModifier<T>
where
    T: serde::Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        #[serde(untagged)]
        enum MapDependencyModifier<T> {
            Package(PackageDependencyModifier<T>),
            Dependency(T),
        }

        serde_untagged::UntaggedEnumVisitor::new()
            .string(|string| T::deserialize(string.into_deserializer()).map(Self::Dependency))
            .map(|map| {
                map.deserialize::<MapDependencyModifier<T>>()
                    .map(|entry| match entry {
                        MapDependencyModifier::Package(package) => Self::Package(package),
                        MapDependencyModifier::Dependency(dependency) => {
                            Self::Dependency(dependency)
                        }
                    })
            })
            .deserialize(deserializer)
    }
}

/// An indexed set of dependency overrides and exclusions.
#[derive(Debug, Default, Clone)]
pub struct DependencyModifiers {
    global_overrides: FxHashMap<PackageName, Vec<Requirement>>,
    global_exclusions: FxHashSet<PackageName>,
    scoped: FxHashMap<PackageName, Vec<ScopedDependencyModifiers>>,
}

#[derive(Debug, Clone)]
struct ScopedDependencyModifiers {
    version: Option<Version>,
    overrides: Option<FxHashMap<PackageName, Vec<Requirement>>>,
    exclusions: Option<FxHashSet<PackageName>>,
}

/// An unsupported source in a scoped dependency override.
#[derive(Debug, thiserror::Error)]
pub enum ScopedOverrideSourceError {
    #[error(
        "Scoped override for `{package}` cannot use a URL or path source for `{dependency}`; scoped overrides currently support version specifiers only"
    )]
    Url {
        package: PackageName,
        dependency: PackageName,
    },
    #[error(
        "Scoped override for `{package}` cannot use an explicit index for `{dependency}`; scoped overrides currently support version specifiers only"
    )]
    Index {
        package: PackageName,
        dependency: PackageName,
    },
}

impl DependencyModifiers {
    /// Create an indexed set of dependency modifiers.
    pub fn from_entries(
        overrides: impl IntoIterator<Item = DependencyModifier<Requirement>>,
        exclusions: impl IntoIterator<Item = DependencyModifier<PackageName>>,
    ) -> Result<Self, ScopedOverrideSourceError> {
        let mut modifiers = Self::default();

        for entry in overrides {
            match entry {
                DependencyModifier::Dependency(requirement) => {
                    modifiers
                        .global_overrides
                        .entry(requirement.name.clone())
                        .or_default()
                        .push(requirement);
                }
                DependencyModifier::Package(package) => {
                    for requirement in &package.dependencies {
                        match &requirement.source {
                            RequirementSource::Registry { index: Some(_), .. } => {
                                return Err(ScopedOverrideSourceError::Index {
                                    package: package.package.name.clone(),
                                    dependency: requirement.name.clone(),
                                });
                            }
                            RequirementSource::Registry { index: None, .. } => {}
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

                    let scoped =
                        modifiers.scoped_mut(package.package.name, package.package.version);
                    let overrides = scoped.overrides.get_or_insert_default();
                    for requirement in package.dependencies {
                        overrides
                            .entry(requirement.name.clone())
                            .or_default()
                            .push(requirement);
                    }
                }
            }
        }

        modifiers.add_exclusions(exclusions);
        Ok(modifiers)
    }

    /// Create dependency modifiers from global override requirements and exclusion entries.
    pub fn from_requirements(
        overrides: impl IntoIterator<Item = Requirement>,
        exclusions: impl IntoIterator<Item = DependencyModifier<PackageName>>,
    ) -> Self {
        let mut modifiers = Self::default();
        for requirement in overrides {
            modifiers
                .global_overrides
                .entry(requirement.name.clone())
                .or_default()
                .push(requirement);
        }
        modifiers.add_exclusions(exclusions);
        modifiers
    }

    fn add_exclusions(
        &mut self,
        exclusions: impl IntoIterator<Item = DependencyModifier<PackageName>>,
    ) {
        for entry in exclusions {
            match entry {
                DependencyModifier::Dependency(dependency) => {
                    self.global_exclusions.insert(dependency);
                }
                DependencyModifier::Package(package) => {
                    let scoped = self.scoped_mut(package.package.name, package.package.version);
                    scoped
                        .exclusions
                        .get_or_insert_default()
                        .extend(package.dependencies);
                }
            }
        }
    }

    fn scoped_mut(
        &mut self,
        package: PackageName,
        version: Option<Version>,
    ) -> &mut ScopedDependencyModifiers {
        let entries = self.scoped.entry(package).or_default();
        let position = entries
            .iter()
            .position(|entry| entry.version == version)
            .unwrap_or_else(|| {
                let position = entries.len();
                entries.push(ScopedDependencyModifiers {
                    version,
                    overrides: None,
                    exclusions: None,
                });
                position
            });
        &mut entries[position]
    }

    /// Return all global override [`Requirement`]s that are not excluded.
    pub fn global_overrides(&self) -> impl Iterator<Item = &Requirement> {
        self.global_overrides
            .values()
            .flatten()
            .filter(|requirement| !self.is_excluded(&requirement.name))
    }

    /// Return all scoped override [`Requirement`]s that are not excluded in their scope.
    pub fn scoped_overrides(
        &self,
    ) -> impl Iterator<Item = (&PackageName, Option<&Version>, &Requirement)> {
        self.scoped.iter().flat_map(move |(package, entries)| {
            entries.iter().flat_map(move |entry| {
                entry
                    .overrides
                    .iter()
                    .flat_map(|overrides| overrides.values().flatten())
                    .filter_map(move |requirement| {
                        (!self.is_excluded_for_scope(
                            package,
                            entry.version.as_ref(),
                            &requirement.name,
                        ))
                        .then_some((
                            package,
                            entry.version.as_ref(),
                            requirement,
                        ))
                    })
            })
        })
    }

    /// Return the scoped override [`Requirement`]s that apply to a specific package version and
    /// are not excluded.
    pub fn scoped_overrides_for(
        &self,
        package: &PackageName,
        version: &Version,
    ) -> impl Iterator<Item = &Requirement> {
        self.scoped_overrides_for_package(package, version)
            .into_iter()
            .flat_map(|scoped| scoped.overrides.iter())
            .flat_map(|overrides| overrides.values().flatten())
            .filter(|requirement| {
                !self.is_excluded_for_package(Some((package, version)), &requirement.name)
            })
    }

    /// Return whether a dependency is globally excluded.
    pub fn is_excluded(&self, dependency: &PackageName) -> bool {
        self.global_exclusions.contains(dependency)
    }

    /// Return whether a dependency is excluded from a specific package version.
    pub fn is_excluded_for(
        &self,
        package: &PackageName,
        version: &Version,
        dependency: &PackageName,
    ) -> bool {
        self.is_excluded_for_package(Some((package, version)), dependency)
    }

    /// Apply all dependency modifiers to a set of requirements.
    ///
    /// NB: Change this method together with [`Constraints::apply`](crate::Constraints::apply).
    pub fn apply<'a, I>(
        &'a self,
        requirements: I,
    ) -> impl Iterator<Item = Cow<'a, Requirement>> + use<'a, I>
    where
        I: IntoIterator<Item = &'a Requirement>,
    {
        self.apply_inner(requirements, None, None)
    }

    /// Apply all dependency modifiers to the dependencies of a specific package version.
    pub fn apply_for<'a, I>(
        &'a self,
        package: &PackageName,
        version: &Version,
        requirements: I,
    ) -> impl Iterator<Item = Cow<'a, Requirement>> + use<'a, I>
    where
        I: IntoIterator<Item = &'a Requirement>,
    {
        self.apply_inner(
            requirements,
            Some((package, version)),
            Some((package, version)),
        )
    }

    /// Apply dependency modifiers with independent package context for overrides and exclusions.
    ///
    /// Dependency-group requirements use package-scoped exclusions but not package-scoped
    /// overrides, while regular package metadata uses the same context for both.
    pub fn apply_for_packages<'a, I>(
        &'a self,
        override_package: Option<(&PackageName, &Version)>,
        exclusion_package: Option<(&PackageName, &Version)>,
        requirements: I,
    ) -> impl Iterator<Item = Cow<'a, Requirement>> + use<'a, I>
    where
        I: IntoIterator<Item = &'a Requirement>,
    {
        self.apply_inner(requirements, override_package, exclusion_package)
    }

    fn apply_inner<'a, I>(
        &'a self,
        requirements: I,
        override_package: Option<(&PackageName, &Version)>,
        exclusion_package: Option<(&PackageName, &Version)>,
    ) -> impl Iterator<Item = Cow<'a, Requirement>> + use<'a, I>
    where
        I: IntoIterator<Item = &'a Requirement>,
    {
        let scoped_overrides = override_package
            .and_then(|(package, version)| self.scoped_overrides_for_package(package, version));
        let scoped_exclusions = exclusion_package
            .and_then(|(package, version)| self.scoped_exclusions_for_package(package, version));
        self.apply_overrides(requirements, scoped_overrides)
            .filter(move |requirement| {
                !self.is_excluded_with_scope(scoped_exclusions, &requirement.name)
            })
    }

    fn apply_overrides<'a, I>(
        &'a self,
        requirements: I,
        scoped: Option<&'a ScopedDependencyModifiers>,
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
                .overrides
                .iter()
                .flat_map(|overrides| overrides.iter())
                .filter(|(name, _)| !names.contains(*name))
                .flat_map(|(_, requirements)| requirements)
                .collect::<Vec<_>>();
            additions.sort_unstable();

            return Either::Left(
                requirements
                    .into_iter()
                    .flat_map(move |requirement| self.apply_requirement(requirement, Some(scoped)))
                    .chain(additions.into_iter().map(Cow::Borrowed)),
            );
        }

        if self.global_overrides.is_empty() {
            return Either::Right(Either::Left(requirements.into_iter().map(Cow::Borrowed)));
        }

        Either::Right(Either::Right(requirements.into_iter().flat_map(
            move |requirement| self.apply_requirement(requirement, None),
        )))
    }

    fn apply_requirement<'a>(
        &'a self,
        requirement: &'a Requirement,
        scoped: Option<&'a ScopedDependencyModifiers>,
    ) -> impl Iterator<Item = Cow<'a, Requirement>> {
        let overrides = scoped
            .and_then(|scoped| scoped.overrides.as_ref())
            .and_then(|overrides| overrides.get(&requirement.name))
            .or_else(|| self.global_overrides.get(&requirement.name));
        let Some(overrides) = overrides else {
            return Either::Left(std::iter::once(Cow::Borrowed(requirement)));
        };

        let Some(extra_expression) = requirement.marker.top_level_extra() else {
            return Either::Right(Either::Right(overrides.iter().map(Cow::Borrowed)));
        };

        Either::Right(Either::Left(overrides.iter().map(
            move |override_requirement| {
                let mut joint_marker = MarkerTree::expression(extra_expression.clone());
                joint_marker.and(override_requirement.marker);
                Cow::Owned(Requirement {
                    marker: joint_marker,
                    ..override_requirement.clone()
                })
            },
        )))
    }

    fn scoped_overrides_for_package(
        &self,
        package: &PackageName,
        version: &Version,
    ) -> Option<&ScopedDependencyModifiers> {
        self.scoped.get(package).and_then(|entries| {
            entries
                .iter()
                .find(|entry| entry.overrides.is_some() && entry.version.as_ref() == Some(version))
                .or_else(|| {
                    entries
                        .iter()
                        .find(|entry| entry.overrides.is_some() && entry.version.is_none())
                })
        })
    }

    fn scoped_exclusions_for_package(
        &self,
        package: &PackageName,
        version: &Version,
    ) -> Option<&ScopedDependencyModifiers> {
        self.scoped.get(package).and_then(|entries| {
            entries
                .iter()
                .find(|entry| entry.exclusions.is_some() && entry.version.as_ref() == Some(version))
                .or_else(|| {
                    entries
                        .iter()
                        .find(|entry| entry.exclusions.is_some() && entry.version.is_none())
                })
        })
    }

    fn has_exact_override_scope(&self, package: &PackageName, version: &Version) -> bool {
        self.scoped.get(package).is_some_and(|entries| {
            entries
                .iter()
                .any(|entry| entry.overrides.is_some() && entry.version.as_ref() == Some(version))
        })
    }

    fn is_excluded_for_scope(
        &self,
        package: &PackageName,
        version: Option<&Version>,
        dependency: &PackageName,
    ) -> bool {
        if let Some(version) = version {
            return self.is_excluded_for_package(Some((package, version)), dependency);
        }
        if self.is_excluded(dependency) {
            return true;
        }

        let Some(entries) = self.scoped.get(package) else {
            return false;
        };
        entries
            .iter()
            .find(|entry| entry.exclusions.is_some() && entry.version.is_none())
            .and_then(|entry| entry.exclusions.as_ref())
            .is_some_and(|exclusions| exclusions.contains(dependency))
            && entries
                .iter()
                .filter(|entry| {
                    entry.exclusions.is_some()
                        && entry
                            .version
                            .as_ref()
                            .is_some_and(|version| !self.has_exact_override_scope(package, version))
                })
                .all(|entry| {
                    entry
                        .exclusions
                        .as_ref()
                        .is_some_and(|exclusions| exclusions.contains(dependency))
                })
    }

    fn is_excluded_for_package(
        &self,
        package: Option<(&PackageName, &Version)>,
        dependency: &PackageName,
    ) -> bool {
        let scoped = package
            .and_then(|(package, version)| self.scoped_exclusions_for_package(package, version));
        self.is_excluded_with_scope(scoped, dependency)
    }

    fn is_excluded_with_scope(
        &self,
        scoped: Option<&ScopedDependencyModifiers>,
        dependency: &PackageName,
    ) -> bool {
        self.is_excluded(dependency)
            || scoped
                .and_then(|scoped| scoped.exclusions.as_ref())
                .is_some_and(|exclusions| exclusions.contains(dependency))
    }
}
