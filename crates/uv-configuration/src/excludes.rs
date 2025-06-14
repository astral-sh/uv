use std::borrow::Cow;

use either::Either;
use rustc_hash::{FxBuildHasher, FxHashMap};

use uv_distribution_types::Requirement;
use uv_normalize::PackageName;
use uv_pep508::MarkerTree;
/// A set of excludes for a set of requirements.
#[derive(Debug, Default, Clone)]
pub struct Excludes(FxHashMap<PackageName, Vec<Requirement>>);

impl Excludes {
    /// Create a new set of excludes from a set of requirements.
    pub fn from_requirements(requirements: Vec<Requirement>) -> Self {
        let mut excludes: FxHashMap<PackageName, Vec<Requirement>> =
            FxHashMap::with_capacity_and_hasher(requirements.len(), FxBuildHasher);
        for requirement in requirements {
            excludes
                .entry(requirement.name.clone())
                .or_default()
                .push(requirement);
        }
        Self(excludes)
    }

    /// Return an iterator over all [`Requirement`]s in the exclude set.
    pub fn requirements(&self) -> impl Iterator<Item = &Requirement> {
        self.0.values().flat_map(|requirements| requirements.iter())
    }

    /// Get the excludes for a package.
    pub fn get(&self, name: &PackageName) -> Option<&Vec<Requirement>> {
        self.0.get(name)
    }

    /// Apply the excludes to a set of requirements.
    ///
    ///  NB: Change this method together with [`Overrides::apply`].
    pub fn apply<'a>(
        &'a self,
        requirements: impl IntoIterator<Item = &'a Requirement>,
    ) -> impl Iterator<Item = Cow<'a, Requirement>> {
        if self.0.is_empty() {
            // Fast path: There are no excludes.
            return Either::Left(requirements.into_iter().map(Cow::Borrowed));
        }

        Either::Right(requirements.into_iter().flat_map(|requirement| {
            let Some(excludes) = self.get(&requirement.name) else {
                // Case 1: No excludes(s).
                return Either::Left(std::iter::once(Cow::Borrowed(requirement)));
            };

            // ASSUMPTION: There is one `extra = "..."`, and it's either the only marker or part
            // of the main conjunction.
            let Some(extra_expression) = requirement.marker.top_level_extra() else {
                // Case 2: A non-optional dependency with exclude(s).
                return Either::Right(Either::Right(excludes.iter().map(Cow::Borrowed)));
            };

            // Case 3: An optional dependency with exclude(s).
            //
            // When the original requirement is an optional dependency, the excludes(s) need to
            // be optional for the same extra, otherwise we activate extras that should be inactive.
            Either::Right(Either::Left(excludes.iter().map(
                move |exclude_requirement| {
                    // Add the extra to the override marker.
                    let mut joint_marker = MarkerTree::expression(extra_expression.clone());
                    joint_marker.and(exclude_requirement.marker);
                    Cow::Owned(Requirement {
                        marker: joint_marker,
                        ..exclude_requirement.clone()
                    })
                },
            )))
        }))
    }
}
