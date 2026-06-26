use std::str::FromStr;

use rustc_hash::{FxHashMap, FxHashSet};
use serde::de::Error;

use uv_normalize::PackageName;
use uv_pep440::Version;

use crate::Overrides;

/// A set of exclusions that applies to the dependencies of a specific package version.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct PackageExclusion {
    pub package: PackageExclusionTarget,
    pub dependencies: Box<[PackageName]>,
}

/// The package and optional version selected by a [`PackageExclusion`].
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct PackageExclusionTarget {
    pub name: PackageName,
    #[cfg_attr(
        feature = "schemars",
        schemars(
            with = "Option<String>",
            description = "PEP 440-style package version, e.g., `1.2.3`"
        )
    )]
    pub version: Option<Version>,
}

/// An exclusion, either global or scoped to a specific package version.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema), schemars(untagged))]
#[serde(untagged)]
pub enum ExcludeDependency {
    Package(PackageExclusion),
    Dependency(PackageName),
}

impl<'de> serde::Deserialize<'de> for ExcludeDependency {
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

/// A set of packages to exclude from resolution.
#[derive(Debug, Default, Clone)]
pub struct Excludes {
    global: FxHashSet<PackageName>,
    scoped: FxHashMap<PackageName, Vec<ScopedExclusions>>,
}

#[derive(Debug, Clone)]
struct ScopedExclusions {
    version: Option<Version>,
    excludes: FxHashSet<PackageName>,
}

impl Excludes {
    /// Get the exclusions for a package with an optional version.
    fn scoped_for(
        &self,
        package: &PackageName,
        version: Option<&Version>,
    ) -> Option<&ScopedExclusions> {
        self.scoped.get(package).and_then(|entries| {
            version
                .and_then(|version| {
                    entries
                        .iter()
                        .find(|entry| entry.version.as_ref() == Some(version))
                })
                .or_else(|| entries.iter().find(|entry| entry.version.is_none()))
        })
    }

    /// Create an indexed set of exclusions.
    pub fn from_entries(entries: impl IntoIterator<Item = ExcludeDependency>) -> Self {
        let mut excludes = Self::default();
        for entry in entries {
            match entry {
                ExcludeDependency::Dependency(dependency) => {
                    excludes.global.insert(dependency);
                }
                ExcludeDependency::Package(package) => {
                    let packages = excludes.scoped.entry(package.package.name).or_default();
                    if let Some(entry) = packages
                        .iter_mut()
                        .find(|entry| entry.version == package.package.version)
                    {
                        entry.excludes.extend(package.dependencies);
                    } else {
                        packages.push(ScopedExclusions {
                            version: package.package.version,
                            excludes: package.dependencies.into_iter().collect(),
                        });
                    }
                }
            }
        }
        excludes
    }

    /// Check if a package is excluded.
    pub fn contains(&self, name: &PackageName) -> bool {
        self.global.contains(name)
    }

    /// Check if a dependency is excluded from a specific package version.
    pub fn contains_for(
        &self,
        package: &PackageName,
        version: &Version,
        dependency: &PackageName,
    ) -> bool {
        self.contains_for_package(Some((package, version)), dependency)
    }

    /// Check if a dependency is always excluded from a package scope.
    ///
    /// A versionless scope remains eligible if any exact-version exclusion allows the dependency
    /// at a version where the override is not shadowed by an exact override scope.
    pub fn contains_for_scope(
        &self,
        overrides: &Overrides,
        package: &PackageName,
        version: Option<&Version>,
        dependency: &PackageName,
    ) -> bool {
        if let Some(version) = version {
            return self.contains_for(package, version, dependency);
        }
        if self.contains(dependency) {
            return true;
        }

        let Some(entries) = self.scoped.get(package) else {
            return false;
        };
        entries
            .iter()
            .find(|entry| entry.version.is_none())
            .is_some_and(|entry| entry.excludes.contains(dependency))
            && entries
                .iter()
                .filter(|entry| {
                    entry
                        .version
                        .as_ref()
                        .is_some_and(|version| !overrides.has_exact_scope(package, version))
                })
                .all(|entry| entry.excludes.contains(dependency))
    }

    /// Check if a dependency is excluded with optional package-version context.
    pub fn contains_for_package(
        &self,
        package: Option<(&PackageName, &Version)>,
        dependency: &PackageName,
    ) -> bool {
        self.contains(dependency)
            || package.is_some_and(|(package, version)| {
                self.scoped_for(package, Some(version))
                    .is_some_and(|entry| entry.excludes.contains(dependency))
            })
    }

    /// Check if a dependency is excluded from a package with an optional version.
    pub fn contains_for_package_scope(
        &self,
        package: &PackageName,
        version: Option<&Version>,
        dependency: &PackageName,
    ) -> bool {
        self.contains(dependency)
            || self
                .scoped_for(package, version)
                .is_some_and(|entry| entry.excludes.contains(dependency))
    }
}

impl FromIterator<PackageName> for Excludes {
    fn from_iter<I: IntoIterator<Item = PackageName>>(iter: I) -> Self {
        Self::from_entries(iter.into_iter().map(ExcludeDependency::Dependency))
    }
}
