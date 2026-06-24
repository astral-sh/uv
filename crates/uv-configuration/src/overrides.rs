use std::borrow::Cow;

use either::Either;
use rustc_hash::{FxBuildHasher, FxHashMap};
use serde::de::IntoDeserializer;

use uv_distribution_types::Requirement;
use uv_normalize::PackageName;
use uv_pep440::Version;
use uv_pep508::MarkerTree;

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
pub struct PackageOverride<T> {
    pub name: PackageName,
    #[cfg_attr(
        feature = "schemars",
        schemars(
            with = "Option<String>",
            description = "PEP 440-style package version, e.g., `1.2.3`"
        )
    )]
    pub version: Option<Version>,
    #[serde(default)]
    pub requires_dist: Box<[T]>,
}

/// An override, either global or scoped to a specific package version.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema), schemars(untagged))]
#[serde(untagged, bound(serialize = "T: serde::Serialize"))]
pub enum Override<T> {
    Package(PackageOverride<T>),
    Requirement(T),
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
            Requirement(T),
        }

        serde_untagged::UntaggedEnumVisitor::new()
            .string(|string| T::deserialize(string.into_deserializer()).map(Self::Requirement))
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

/// A set of overrides for a set of requirements.
#[derive(Debug, Default, Clone)]
pub struct Overrides {
    global: FxHashMap<PackageName, Vec<Requirement>>,
    scoped: FxHashMap<PackageName, Vec<ScopedOverrides>>,
}

#[derive(Debug, Clone)]
struct ScopedOverrides {
    version: Option<Version>,
    overrides: FxHashMap<PackageName, Vec<Requirement>>,
}

impl Overrides {
    /// Create a new set of overrides from a set of requirements.
    pub fn from_requirements(requirements: Vec<Requirement>) -> Self {
        Self::from_entries(
            requirements
                .into_iter()
                .map(Override::Requirement)
                .collect(),
        )
    }

    /// Create an indexed set of overrides.
    pub fn from_entries(entries: Vec<Override<Requirement>>) -> Self {
        let mut global: FxHashMap<PackageName, Vec<Requirement>> =
            FxHashMap::with_capacity_and_hasher(entries.len(), FxBuildHasher);
        let mut scoped: FxHashMap<PackageName, Vec<ScopedOverrides>> = FxHashMap::default();

        for entry in entries {
            match entry {
                Override::Requirement(requirement) => {
                    global
                        .entry(requirement.name.clone())
                        .or_default()
                        .push(requirement);
                }
                Override::Package(package) => {
                    let packages = scoped.entry(package.name.clone()).or_default();
                    let position = packages
                        .iter()
                        .position(|overrides| overrides.version == package.version)
                        .unwrap_or_else(|| {
                            let position = packages.len();
                            packages.push(ScopedOverrides {
                                version: package.version,
                                overrides: FxHashMap::default(),
                            });
                            position
                        });
                    let overrides = &mut packages[position].overrides;
                    for requirement in package.requires_dist {
                        overrides
                            .entry(requirement.name.clone())
                            .or_default()
                            .push(requirement);
                    }
                }
            }
        }

        Self { global, scoped }
    }

    /// Return an iterator over all [`Requirement`]s in the override set.
    pub fn requirements(&self) -> impl Iterator<Item = &Requirement> {
        self.global
            .values()
            .flat_map(|requirements| requirements.iter())
            .chain(
                self.scoped
                    .values()
                    .flatten()
                    .flat_map(|scoped| scoped.overrides.values().flatten()),
            )
    }

    /// Get the overrides for a package.
    fn get(&self, name: &PackageName) -> Option<&Vec<Requirement>> {
        self.global.get(name)
    }

    /// Get the overrides for a specific package version.
    fn scoped_for(&self, package: &PackageName, version: &Version) -> Option<&ScopedOverrides> {
        self.scoped.get(package).and_then(|entries| {
            entries
                .iter()
                .find(|entry| entry.version.as_ref() == Some(version))
                .or_else(|| entries.iter().find(|entry| entry.version.is_none()))
        })
    }

    /// Apply the overrides to a set of requirements.
    ///
    /// NB: Change this method together with [`Constraints::apply`].
    pub fn apply<'a, I>(
        &'a self,
        requirements: I,
    ) -> impl Iterator<Item = Cow<'a, Requirement>> + use<'a, I>
    where
        I: IntoIterator<Item = &'a Requirement>,
    {
        self.apply_inner(requirements, None)
    }

    /// Apply the overrides to the dependencies of a specific package version.
    pub fn apply_for<'a, I>(
        &'a self,
        package: &PackageName,
        version: &Version,
        requirements: I,
    ) -> impl Iterator<Item = Cow<'a, Requirement>> + use<'a, I>
    where
        I: IntoIterator<Item = &'a Requirement>,
    {
        self.apply_inner(requirements, Some((package, version)))
    }

    /// Apply overrides with optional package-version context.
    pub fn apply_for_package<'a, I>(
        &'a self,
        package: Option<(&PackageName, &Version)>,
        requirements: I,
    ) -> impl Iterator<Item = Cow<'a, Requirement>> + use<'a, I>
    where
        I: IntoIterator<Item = &'a Requirement>,
    {
        self.apply_inner(requirements, package)
    }

    fn apply_inner<'a, I>(
        &'a self,
        requirements: I,
        package: Option<(&PackageName, &Version)>,
    ) -> impl Iterator<Item = Cow<'a, Requirement>> + use<'a, I>
    where
        I: IntoIterator<Item = &'a Requirement>,
    {
        let scoped = package.and_then(|(package, version)| self.scoped_for(package, version));
        if self.global.is_empty() && scoped.is_none() {
            // Fast path: There are no overrides.
            return Either::Left(requirements.into_iter().map(Cow::Borrowed));
        }

        Either::Right(requirements.into_iter().flat_map(move |requirement| {
            let overrides = scoped
                .and_then(|scoped| scoped.overrides.get(&requirement.name))
                .or_else(|| self.get(&requirement.name));
            let Some(overrides) = overrides else {
                // Case 1: No override(s).
                return Either::Left(std::iter::once(Cow::Borrowed(requirement)));
            };

            // ASSUMPTION: There is one `extra = "..."`, and it's either the only marker or part
            // of the main conjunction.
            let Some(extra_expression) = requirement.marker.top_level_extra() else {
                // Case 2: A non-optional dependency with override(s).
                return Either::Right(Either::Right(overrides.iter().map(Cow::Borrowed)));
            };

            // Case 3: An optional dependency with override(s).
            //
            // When the original requirement is an optional dependency, the override(s) need to
            // be optional for the same extra, otherwise we activate extras that should be inactive.
            Either::Right(Either::Left(overrides.iter().map(
                move |override_requirement| {
                    // Add the extra to the override marker.
                    let mut joint_marker = MarkerTree::expression(extra_expression.clone());
                    joint_marker.and(override_requirement.marker);
                    Cow::Owned(Requirement {
                        marker: joint_marker,
                        ..override_requirement.clone()
                    })
                },
            )))
        }))
    }
}
