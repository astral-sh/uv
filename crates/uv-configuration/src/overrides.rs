use serde::de::IntoDeserializer;

use uv_distribution_types::Requirement;
use uv_normalize::PackageName;

use crate::PackageDependencyModifierTarget;

/// An override that applies to the dependencies of a specific package version.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(
    rename_all = "kebab-case",
    deny_unknown_fields,
    bound(
        serialize = "T: serde::Serialize",
        deserialize = "T: serde::Deserialize<'de>"
    )
)]
pub struct PackageOverride<T = Requirement> {
    pub package: PackageDependencyModifierTarget,
    pub dependencies: Box<[T]>,
}

/// An override, either global or scoped to a specific package version.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema), schemars(untagged))]
#[serde(untagged, bound(serialize = "T: serde::Serialize"))]
pub enum Override<T = Requirement> {
    Package(PackageOverride<T>),
    Requirement(Box<T>),
}

impl<T> Override<T> {
    /// Create a global dependency override.
    pub fn requirement(requirement: T) -> Self {
        Self::Requirement(Box::new(requirement))
    }

    /// Map the requirements in this override.
    pub fn map_requirements<U>(self, mut function: impl FnMut(T) -> U) -> Override<U> {
        match self {
            Self::Package(package) => Override::Package(PackageOverride {
                package: package.package,
                dependencies: package
                    .dependencies
                    .into_vec()
                    .into_iter()
                    .map(function)
                    .collect(),
            }),
            Self::Requirement(requirement) => Override::requirement(function(*requirement)),
        }
    }

    /// Fallibly map the requirements in this override.
    pub fn try_map_requirements<E>(
        self,
        mut function: impl FnMut(T) -> Result<T, E>,
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
            Self::Requirement(requirement) => Self::requirement(function(*requirement)?),
        })
    }
}

// A derived `#[serde(untagged)]` implementation collapses detailed requirement parse errors into
// "data did not match any variant", so use a type-directed visitor for string requirements.
impl<'de, T> serde::Deserialize<'de> for Override<T>
where
    T: serde::Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        #[serde(untagged)]
        enum MapOverride<T> {
            Package(PackageOverride<T>),
            Requirement(Box<T>),
        }

        serde_untagged::UntaggedEnumVisitor::new()
            .string(|string| T::deserialize(string.into_deserializer()).map(Self::requirement))
            .map(|map| {
                map.deserialize::<MapOverride<T>>()
                    .map(|entry| match entry {
                        MapOverride::Package(package) => Self::Package(package),
                        MapOverride::Requirement(requirement) => Self::Requirement(requirement),
                    })
            })
            .deserialize(deserializer)
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
