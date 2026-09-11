use std::str::FromStr;

use serde::de::Error;

use uv_normalize::PackageName;

use crate::PackageDependencyModifierTarget;

/// A set of exclusions that applies to the dependencies of a specific package version.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct PackageExclusion {
    pub(crate) package: PackageDependencyModifierTarget,
    pub(crate) dependencies: Box<[PackageName]>,
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
