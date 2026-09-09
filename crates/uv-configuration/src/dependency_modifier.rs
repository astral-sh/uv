use std::fmt;

use serde::de::Error;

use uv_normalize::PackageName;
use uv_pep440::Version;

use crate::{ExcludeDependency, Override, ScopedOverrideSourceError};

mod index;

use index::DependencyModifierIndex;

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
}
