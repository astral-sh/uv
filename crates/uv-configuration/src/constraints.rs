use std::borrow::Cow;

use either::Either;
use rustc_hash::FxHashMap;

use uv_distribution_types::{NameRequirementSpecification, Requirement, RequirementSource};
use uv_normalize::PackageName;
use uv_pep508::MarkerTree;

/// A set of constraints for a set of requirements.
#[derive(Debug, Default, Clone)]
pub struct Constraints {
    /// Original declarations, before removing extras or empty constraints.
    specifications: Vec<NameRequirementSpecification>,
    /// Constraints grouped by package name for resolution.
    requirements: FxHashMap<PackageName, Vec<Requirement>>,
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
            let Some(extra_expression) = requirement.marker.top_level_extra() else {
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
                    let joint_marker =
                        MarkerTree::expression(extra_expression.clone()).and(constraint.marker);
                    Cow::Owned(Requirement {
                        marker: joint_marker,
                        ..constraint
                    })
                }),
            )))
        })
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;

    use uv_distribution_types::{NameRequirementSpecification, Requirement};
    use uv_pep508::Requirement as Pep508Requirement;

    use super::Constraints;

    #[test]
    fn preserve_specifications() -> Result<()> {
        let specifications = [
            NameRequirementSpecification {
                requirement: Requirement::from("foo[bar]".parse::<Pep508Requirement<_>>()?),
                hashes: vec!["sha256:abc".to_string()],
            },
            NameRequirementSpecification {
                requirement: Requirement::from(
                    "baz[qux]==1 ; python_version >= '3.12'".parse::<Pep508Requirement<_>>()?,
                ),
                hashes: vec!["sha256:def".to_string(), "sha512:abc".to_string()],
            },
            NameRequirementSpecification::from(Requirement::from(
                "baz<2".parse::<Pep508Requirement<_>>()?,
            )),
        ];
        let constraints = Constraints::from_specifications(specifications.clone());

        assert_eq!(
            constraints.specifications().cloned().collect::<Vec<_>>(),
            specifications,
        );
        assert!(constraints.get(&"foo".parse()?).is_none());
        insta::assert_debug_snapshot!(
            constraints.requirements().map(ToString::to_string).collect::<Vec<_>>(),
            @r#"
        [
            "baz==1 ; python_full_version >= '3.12'",
            "baz<2",
        ]
        "#
        );

        Ok(())
    }
}
