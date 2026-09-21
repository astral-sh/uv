use std::borrow::Cow;

use rustc_hash::FxHashMap;
use serde::de::IntoDeserializer;

use uv_distribution_types::{NameRequirementSpecification, Requirement, RequirementSource};
use uv_normalize::PackageName;
use uv_pep440::Version;

/// A constraint that applies to the dependencies of a specific package version.
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
pub struct PackageConstraint<T> {
    pub package: PackageConstraintTarget,
    pub dependencies: Box<[T]>,
}

/// The package and optional version selected by a [`PackageConstraint`].
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct PackageConstraintTarget {
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

/// A constraint, either global or scoped to a specific package version.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema), schemars(untagged))]
#[serde(untagged, bound(serialize = "T: serde::Serialize"))]
pub enum Constraint<T> {
    Package(PackageConstraint<T>),
    Requirement(T),
}

// A derived `#[serde(untagged)]` implementation collapses detailed requirement parse errors into
// "data did not match any variant", so use a type-directed visitor for string requirements.
impl<'de, T> serde::Deserialize<'de> for Constraint<T>
where
    T: serde::Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        #[serde(untagged)]
        enum MapConstraint<T> {
            Package(PackageConstraint<T>),
            Requirement(T),
        }

        serde_untagged::UntaggedEnumVisitor::new()
            .string(|string| T::deserialize(string.into_deserializer()).map(Self::Requirement))
            .map(|map| {
                map.deserialize::<MapConstraint<T>>()
                    .map(|entry| match entry {
                        MapConstraint::Package(package) => Self::Package(package),
                        MapConstraint::Requirement(requirement) => Self::Requirement(requirement),
                    })
            })
            .deserialize(deserializer)
    }
}

impl<T> Constraint<T> {
    /// Transform each requirement while retaining its package selector.
    pub fn map<U>(self, mut f: impl FnMut(T) -> U) -> Constraint<U> {
        match self {
            Self::Requirement(requirement) => Constraint::Requirement(f(requirement)),
            Self::Package(package) => Constraint::Package(PackageConstraint {
                package: package.package,
                dependencies: package.dependencies.into_vec().into_iter().map(f).collect(),
            }),
        }
    }

    /// Transform each requirement, propagating errors without changing the selector.
    pub fn try_map<U, E>(self, mut f: impl FnMut(T) -> Result<U, E>) -> Result<Constraint<U>, E> {
        Ok(match self {
            Self::Requirement(requirement) => Constraint::Requirement(f(requirement)?),
            Self::Package(package) => Constraint::Package(PackageConstraint {
                package: package.package,
                dependencies: package
                    .dependencies
                    .into_vec()
                    .into_iter()
                    .map(f)
                    .collect::<Result<_, _>>()?,
            }),
        })
    }

    /// Return a requirement only if its declaration is global.
    pub fn as_requirement(&self) -> Option<&T> {
        match self {
            Self::Requirement(requirement) => Some(requirement),
            Self::Package(_) => None,
        }
    }
}

impl<T> From<T> for Constraint<T> {
    fn from(requirement: T) -> Self {
        Self::Requirement(requirement)
    }
}

impl PackageConstraintTarget {
    /// Whether this selector applies to the given package version.
    pub fn matches(&self, name: &PackageName, version: Option<&Version>) -> bool {
        self.name == *name
            && self
                .version
                .as_ref()
                .is_none_or(|expected| Some(expected) == version)
    }
}

/// An unsupported source in a scoped dependency constraint.
#[derive(Debug, thiserror::Error)]
#[error(
    "Scoped constraint for `{package}` cannot use a URL, path, or explicit index for `{dependency}`; scoped constraints currently support version specifiers only"
)]
pub struct ScopedConstraintSourceError {
    package: PackageName,
    dependency: PackageName,
}

/// A set of constraints for a set of requirements.
#[derive(Debug, Default, Clone)]
pub struct Constraints {
    /// Original declarations, including hashes, for hash verification.
    specifications: Vec<NameRequirementSpecification>,
    /// Constraints grouped by package name.
    requirements: FxHashMap<PackageName, Vec<Requirement>>,
    scoped: FxHashMap<PackageName, Vec<(PackageConstraintTarget, Requirement)>>,
}

impl Constraints {
    /// Create a new set of constraints from a set of requirements.
    pub fn from_requirements(requirements: impl Iterator<Item = Requirement>) -> Self {
        Self::from_specifications(requirements.map(NameRequirementSpecification::from))
    }

    /// Create constraints while retaining their hashes and original declarations.
    pub fn from_specifications(
        specifications: impl IntoIterator<Item = NameRequirementSpecification>,
    ) -> Self {
        let specifications: Vec<_> = specifications.into_iter().collect();
        let mut constraints: FxHashMap<PackageName, Vec<Requirement>> = FxHashMap::default();
        for specification in &specifications {
            let requirement = &specification.requirement;
            // Skip empty constraints.
            if let RequirementSource::Registry { specifier, .. } = &requirement.source
                && specifier.is_empty()
            {
                continue;
            }

            constraints
                .entry(requirement.name.clone())
                .or_default()
                .push(Requirement {
                    // We add and apply constraints independent of their extras.
                    extras: Box::new([]),
                    ..requirement.clone()
                });
        }
        Self {
            specifications,
            requirements: constraints,
            scoped: FxHashMap::default(),
        }
    }

    /// Return the original declarations, including hashes, in input order.
    pub fn specifications(&self) -> impl Iterator<Item = &NameRequirementSpecification> {
        self.specifications.iter()
    }

    /// Return an iterator over all [`Requirement`]s in the constraint set.
    pub fn requirements(&self) -> impl Iterator<Item = &Requirement> {
        self.requirements.values().flatten()
    }

    /// Get the constraints for a package.
    pub fn get(&self, name: &PackageName) -> Option<&Vec<Requirement>> {
        self.requirements.get(name)
    }

    /// Add constraints scoped to the package declaring a dependency.
    fn with_scoped(
        mut self,
        scopes: impl IntoIterator<Item = PackageConstraint<Requirement>>,
    ) -> Result<Self, ScopedConstraintSourceError> {
        for mut scope in scopes {
            for requirement in &mut scope.dependencies {
                if !matches!(
                    requirement.source,
                    RequirementSource::Registry { index: None, .. }
                ) {
                    return Err(ScopedConstraintSourceError {
                        package: scope.package.name.clone(),
                        dependency: requirement.name.clone(),
                    });
                }
                requirement.extras = Box::new([]);
            }
            for requirement in scope.dependencies {
                self.scoped
                    .entry(requirement.name.clone())
                    .or_default()
                    .push((scope.package.clone(), requirement));
            }
        }
        Ok(self)
    }

    /// Create constraints from global and package-scoped declarations.
    pub fn from_entries(
        entries: impl IntoIterator<Item = Constraint<NameRequirementSpecification>>,
    ) -> Result<Self, ScopedConstraintSourceError> {
        let mut global = Vec::new();
        let mut scoped = Vec::new();
        for entry in entries {
            match entry {
                Constraint::Requirement(requirement) => global.push(requirement),
                Constraint::Package(package) => scoped.push(PackageConstraint {
                    package: package.package,
                    dependencies: package
                        .dependencies
                        .into_vec()
                        .into_iter()
                        .map(|spec| spec.requirement)
                        .collect(),
                }),
            }
        }
        Self::from_specifications(global).with_scoped(scoped)
    }

    /// Get all constraints on a dependency in the given parent package.
    pub fn get_for<'a>(
        &'a self,
        package: Option<(&PackageName, &Version)>,
        name: &PackageName,
    ) -> impl Iterator<Item = &'a Requirement> + use<'a> {
        let package = package.map(|(name, version)| (name.clone(), version.clone()));
        self.get(name).into_iter().flatten().chain(
            self.scoped
                .get(name)
                .into_iter()
                .flatten()
                .filter(move |(scope, _)| {
                    package
                        .as_ref()
                        .is_some_and(|(name, version)| scope.matches(name, Some(version)))
                })
                .map(|(_, requirement)| requirement),
        )
    }

    /// Return scoped constraints and their selectors for candidate selection policy.
    pub fn scoped_requirements(
        &self,
    ) -> impl Iterator<Item = (&PackageName, Option<&Version>, &Requirement)> {
        self.scoped
            .values()
            .flatten()
            .map(|(scope, requirement)| (&scope.name, scope.version.as_ref(), requirement))
    }

    /// Apply global constraints to requirements without a parent package.
    pub fn apply<'a, I>(
        &'a self,
        requirements: I,
    ) -> impl Iterator<Item = Cow<'a, Requirement>> + use<'a, I>
    where
        I: IntoIterator<Item = Cow<'a, Requirement>>,
    {
        self.apply_for_package(None, requirements)
    }

    /// Apply global and matching scoped constraints to dependency requirements.
    pub fn apply_for_package<'a, I>(
        &'a self,
        package: Option<(&PackageName, &Version)>,
        requirements: I,
    ) -> impl Iterator<Item = Cow<'a, Requirement>> + use<'a, I>
    where
        I: IntoIterator<Item = Cow<'a, Requirement>>,
    {
        let package = package.map(|(name, version)| (name.clone(), version.clone()));
        requirements.into_iter().flat_map(move |requirement| {
            let marker = requirement.marker;
            let constraints = self.get_for(
                package.as_ref().map(|(name, version)| (name, version)),
                &requirement.name,
            );
            std::iter::once(requirement).chain(constraints.map(move |constraint| {
                // A constraint only applies where the original dependency is active.
                let marker = marker.and(constraint.marker);
                if marker == constraint.marker {
                    Cow::Borrowed(constraint)
                } else {
                    Cow::Owned(Requirement {
                        marker,
                        ..constraint.clone()
                    })
                }
            }))
        })
    }
}
