use std::borrow::Cow;

use either::Either;
use rustc_hash::FxHashMap;

use uv_distribution_types::{
    NameRequirementSpecification, Requirement, RequirementSource, ResolutionRecorder,
};
use uv_normalize::PackageName;
use uv_pep508::MarkerTree;

/// A set of constraints for a set of requirements.
#[derive(Debug, Default, Clone)]
pub struct Constraints {
    recorder: Option<ResolutionRecorder>,
    /// Original declarations, including hashes, for hash verification.
    specifications: Vec<NameRequirementSpecification>,
    /// Constraints grouped by package name.
    requirements: FxHashMap<PackageName, Vec<Requirement>>,
}

impl Constraints {
    /// Record which settings are consulted while resolving runtime dependencies.
    #[must_use]
    pub fn with_recorder(mut self, recorder: Option<ResolutionRecorder>) -> Self {
        self.recorder = recorder;
        self
    }

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
            recorder: None,
            specifications,
            requirements: constraints,
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
        if let Some(recorder) = &self.recorder {
            recorder.constraint(name);
        }
        self.requirements.get(name)
    }

    /// Apply the constraints to a set of requirements.
    ///
    /// NB: Change this method together with `Overrides::apply_for_package`.
    pub fn apply<'a>(
        &'a self,
        requirements: impl IntoIterator<Item = Cow<'a, Requirement>>,
    ) -> impl Iterator<Item = Cow<'a, Requirement>> {
        requirements.into_iter().flat_map(|requirement| {
            let Some(constraints) = self.get(&requirement.name) else {
                // Case 1: No constraint(s).
                return Either::Left(std::iter::once(requirement));
            };

            // Constraints must retain the requirement's full extra condition, including
            // alternatives and negative conditions. A false marker has no extras to retain.
            let extra_marker = if requirement.marker.is_false() {
                MarkerTree::TRUE
            } else {
                requirement.marker.only_extras()
            };
            if extra_marker.is_true() {
                // Case 2: A non-optional dependency with constraint(s).
                return Either::Right(Either::Right(
                    std::iter::once(requirement).chain(constraints.iter().map(Cow::Borrowed)),
                ));
            }

            // Case 3: An optional dependency with constraint(s).
            //
            // When the original requirement is an optional dependency, the constraint(s) need to
            // have the same extra condition, otherwise we activate extras that should be inactive.
            Either::Right(Either::Left(std::iter::once(requirement).chain(
                constraints.iter().cloned().map(move |constraint| {
                    let joint_marker = extra_marker.and(constraint.marker);
                    Cow::Owned(Requirement {
                        marker: joint_marker,
                        ..constraint
                    })
                }),
            )))
        })
    }
}
