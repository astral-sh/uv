use std::fmt;
use std::str::FromStr;

use serde::de::{Error, IntoDeserializer};

use uv_distribution_types::Requirement;
use uv_normalize::PackageName;
use uv_pep440::Version;

mod index;

use index::DependencyModifierIndex;

/// Overrides that apply to the dependencies of a specific package version.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct PackageOverride {
    pub package: PackageDependencyModifierTarget,
    pub dependencies: Box<[Requirement]>,
}

/// A set of exclusions that applies to the dependencies of a specific package version.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct PackageExclusion {
    package: PackageDependencyModifierTarget,
    dependencies: Box<[PackageName]>,
}

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

/// A dependency override, either global or scoped to a specific package version.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[serde(untagged)]
pub enum DependencyOverride {
    Package(PackageOverride),
    Requirement(Box<Requirement>),
}

impl DependencyOverride {
    /// Create a global dependency override.
    pub fn requirement(requirement: Requirement) -> Self {
        Self::Requirement(Box::new(requirement))
    }

    /// Fallibly map the requirements in this override.
    pub fn try_map_requirements<E>(
        self,
        mut function: impl FnMut(Requirement) -> Result<Requirement, E>,
    ) -> Result<Self, E> {
        Ok(match self {
            Self::Package(package) => Self::Package(PackageOverride {
                package: package.package,
                dependencies: package
                    .dependencies
                    .into_vec()
                    .into_iter()
                    .map(function)
                    .collect::<Result<Box<[_]>, _>>()?,
            }),
            Self::Requirement(requirement) => Self::Requirement(Box::new(function(*requirement)?)),
        })
    }
}

/// An exclusion, either global or scoped to a specific package version.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[cfg_attr(
    feature = "schemars",
    derive(schemars::JsonSchema),
    schemars(untagged, rename = "ExcludeDependency")
)]
#[serde(untagged)]
pub enum DependencyExclusion {
    Package(PackageExclusion),
    Dependency(PackageName),
}

// A derived `#[serde(untagged)]` implementation collapses detailed dependency parse errors into
// "data did not match any variant", so use a type-directed visitor for string dependencies.
impl<'de> serde::Deserialize<'de> for DependencyOverride {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        #[serde(untagged)]
        enum MapDependencyOverride {
            Package(PackageOverride),
            Requirement(Box<Requirement>),
        }

        serde_untagged::UntaggedEnumVisitor::new()
            .string(|string| {
                Requirement::deserialize(string.into_deserializer())
                    .map(Box::new)
                    .map(Self::Requirement)
            })
            .map(|map| {
                map.deserialize::<MapDependencyOverride>()
                    .map(|entry| match entry {
                        MapDependencyOverride::Package(package) => Self::Package(package),
                        MapDependencyOverride::Requirement(requirement) => {
                            Self::Requirement(requirement)
                        }
                    })
            })
            .deserialize(deserializer)
    }
}

impl<'de> serde::Deserialize<'de> for DependencyExclusion {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        serde_untagged::UntaggedEnumVisitor::new()
            .string(|string| {
                PackageName::from_str(string)
                    .map(Self::Dependency)
                    .map_err(Error::custom)
            })
            .map(|map| map.deserialize().map(Self::Package))
            .deserialize(deserializer)
    }
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
    overrides: Vec<DependencyOverride>,
    #[serde(default, rename = "excludes")]
    exclusions: Vec<DependencyExclusion>,
}

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
    /// Return whether there are no dependency modifiers.
    pub fn is_empty(&self) -> bool {
        self.entries.overrides.is_empty() && self.entries.exclusions.is_empty()
    }

    /// Return the override entries.
    pub fn override_entries(&self) -> impl Iterator<Item = &DependencyOverride> {
        self.entries.overrides.iter()
    }

    /// Return the exclusion entries.
    pub fn exclusion_entries(&self) -> impl Iterator<Item = &DependencyExclusion> {
        self.entries.exclusions.iter()
    }

    /// Consume the collection and return its override and exclusion entries.
    pub fn into_parts(self) -> (Vec<DependencyOverride>, Vec<DependencyExclusion>) {
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
        overrides: impl IntoIterator<Item = DependencyOverride>,
        exclusions: impl IntoIterator<Item = DependencyExclusion>,
    ) -> Result<Self, ScopedOverrideSourceError> {
        let mut modifiers = Self::default();
        modifiers.extend_overrides(overrides)?;
        modifiers.extend_exclusions(exclusions);
        Ok(modifiers)
    }

    /// Add override entries to this collection.
    pub fn extend_overrides(
        &mut self,
        overrides: impl IntoIterator<Item = DependencyOverride>,
    ) -> Result<(), ScopedOverrideSourceError> {
        for entry in overrides {
            self.index.insert_override(&entry)?;
            self.entries.overrides.push(entry);
        }
        Ok(())
    }

    /// Add exclusion entries to this collection.
    pub fn extend_exclusions(&mut self, exclusions: impl IntoIterator<Item = DependencyExclusion>) {
        for entry in exclusions {
            self.index.insert_exclusion(&entry);
            self.entries.exclusions.push(entry);
        }
    }
}
