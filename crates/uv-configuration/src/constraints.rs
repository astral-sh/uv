use std::borrow::Cow;

use either::Either;
use rustc_hash::FxHashMap;

use pep508_rs::MarkerTree;
use pypi_types::{Requirement, RequirementSource};
use uv_normalize::PackageName;

/// A set of constraints for a set of requirements.
#[derive(Debug, Default, Clone)]
pub struct Constraints(FxHashMap<PackageName, Vec<Requirement>>);

impl Constraints {
    /// Create a new set of constraints from a set of requirements.
    pub fn from_requirements(requirements: impl Iterator<Item = Requirement>) -> Self {
        let mut constraints: FxHashMap<PackageName, Vec<Requirement>> = FxHashMap::default();
        for requirement in requirements {
            // Skip empty constraints.
            if let RequirementSource::Registry { specifier, .. } = &requirement.source {
                if specifier.is_empty() {
                    continue;
                }
            }

            constraints
                .entry(requirement.name.clone())
                .or_default()
                .push(Requirement {
                    // We add and apply constraints independent of their extras.
                    extras: vec![],
                    ..requirement
                });
        }
        Self(constraints)
    }

    /// Return an iterator over all [`Requirement`]s in the constraint set.
    pub fn requirements(&self) -> impl Iterator<Item = &Requirement> {
        self.0.values().flat_map(|requirements| requirements.iter())
    }

    /// Get the constraints for a package.
    pub fn get(&self, name: &PackageName) -> Option<&Vec<Requirement>> {
        self.0.get(name)
    }

    /// Apply the constraints to a set of requirements.
    ///
    /// NB: Change this method together with [`Overrides::apply`].
    pub fn apply<'a>(
        &'a self,
        requirements: impl IntoIterator<Item = Cow<'a, Requirement>>,
    ) -> impl Iterator<Item = Cow<'a, Requirement>> {
        requirements.into_iter().flat_map(|requirement| {
            let Some(constraints) = self.get(&requirement.name) else {
                // Case 1: No constraint(s).
                return Either::Left(std::iter::once(requirement));
            };

            // ASSUMPTION: There is one `extra = "..."`, and it's either the only marker or part
            // of the main conjunction.
            let Some(extra_expression) = requirement
                .marker
                .as_ref()
                .and_then(|marker| marker.top_level_extra())
                .cloned()
            else {
                // Case 2: A non-optional dependency with constraint(s).
                return Either::Right(Either::Right(
                    std::iter::once(requirement).chain(constraints.iter().map(Cow::Borrowed)),
                ));
            };

            // Case 3: An optional dependency with constraint(s).
            //
            // When the original requirement is an optional dependency, the constraint(s) need to
            // be optional for the same extra, otherwise we activate extras that should be inactive.
            Either::Right(Either::Left(std::iter::once(requirement).chain(
                constraints.iter().cloned().map(move |constraint| {
                    // Add the extra to the override marker.
                    let mut joint_marker = MarkerTree::Expression(extra_expression.clone());
                    if let Some(marker) = &constraint.marker {
                        joint_marker.and(marker.clone());
                    }
                    Cow::Owned(Requirement {
                        marker: Some(joint_marker.clone()),
                        ..constraint
                    })
                }),
            )))
        })
    }
}
