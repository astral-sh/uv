use std::borrow::Cow;

use either::Either;
use rustc_hash::{FxBuildHasher, FxHashMap};

use pep508_rs::MarkerTree;
use pypi_types::Requirement;
use uv_normalize::PackageName;

/// A set of overrides for a set of requirements.
#[derive(Debug, Default, Clone)]
pub struct Overrides(FxHashMap<PackageName, Vec<Requirement>>);

impl Overrides {
    /// Create a new set of overrides from a set of requirements.
    pub fn from_requirements(requirements: Vec<Requirement>) -> Self {
        let mut overrides: FxHashMap<PackageName, Vec<Requirement>> =
            FxHashMap::with_capacity_and_hasher(requirements.len(), FxBuildHasher);
        for requirement in requirements {
            overrides
                .entry(requirement.name.clone())
                .or_default()
                .push(requirement);
        }
        Self(overrides)
    }

    /// Return an iterator over all [`Requirement`]s in the override set.
    pub fn requirements(&self) -> impl Iterator<Item = &Requirement> {
        self.0.values().flat_map(|requirements| requirements.iter())
    }

    /// Get the overrides for a package.
    pub fn get(&self, name: &PackageName) -> Option<&Vec<Requirement>> {
        self.0.get(name)
    }

    /// Apply the overrides to a set of requirements.
    ///
    /// NB: Change this method together with [`Constraints::apply`].
    pub fn apply<'a>(
        &'a self,
        requirements: impl IntoIterator<Item = &'a Requirement>,
    ) -> impl Iterator<Item = Cow<'a, Requirement>> {
        requirements.into_iter().flat_map(|requirement| {
            let Some(overrides) = self.get(&requirement.name) else {
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
                    joint_marker.and(override_requirement.marker.clone());
                    Cow::Owned(Requirement {
                        marker: joint_marker,
                        ..override_requirement.clone()
                    })
                },
            )))
        })
    }
}
