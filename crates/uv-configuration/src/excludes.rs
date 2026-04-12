use std::borrow::Cow;
use std::collections::BTreeSet;

use thiserror::Error;
use uv_distribution_types::{Requirement, RequirementSource};
use uv_normalize::PackageName;
use uv_pep440::VersionSpecifiers;
use uv_pep508::MarkerTree;

/// A set of packages to exclude from resolution.
#[derive(Debug, Default, Clone)]
pub struct Excludes(BTreeSet<Requirement>);

#[derive(Debug, Error)]
#[error(
    "`exclude-dependencies` entry `{0}` must be a package name with optional environment markers; version specifiers, extras, and direct URLs are not allowed"
)]
pub struct ExcludesError(String);

impl Excludes {
    /// Create an exclusion set from validated requirement entries.
    pub fn from_requirements(
        requirements: impl IntoIterator<Item = Requirement>,
    ) -> Result<Self, ExcludesError> {
        requirements
            .into_iter()
            .map(|requirement| {
                if !requirement.extras.is_empty()
                    || !requirement.groups.is_empty()
                    || !matches!(
                        &requirement.source,
                        RequirementSource::Registry { specifier, .. } if specifier.is_empty()
                    )
                {
                    return Err(ExcludesError(requirement.to_string()));
                }
                Ok(requirement)
            })
            .collect::<Result<BTreeSet<_>, _>>()
            .map(Self)
    }

    /// Return an iterator over all exclusions.
    pub fn iter(&self) -> impl Iterator<Item = &Requirement> {
        self.0.iter()
    }

    /// Return `true` if no exclusions are present.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Apply any exclusions that target the given requirement.
    pub fn apply<'a>(&self, requirement: Cow<'a, Requirement>) -> Option<Cow<'a, Requirement>> {
        let mut marker = requirement.as_ref().marker;
        let mut changed = false;

        for exclude in self
            .0
            .iter()
            .filter(|exclude| exclude.name == requirement.as_ref().name)
        {
            marker.and(exclude.marker.negate());
            changed = true;
            if marker.is_false() {
                return None;
            }
        }

        if !changed {
            return Some(requirement);
        }

        let mut requirement = requirement.into_owned();
        requirement.marker = marker;
        Some(Cow::Owned(requirement))
    }
}

impl FromIterator<PackageName> for Excludes {
    fn from_iter<I: IntoIterator<Item = PackageName>>(iter: I) -> Self {
        Self(
            iter.into_iter()
                .map(|name| Requirement {
                    name,
                    extras: Vec::new().into_boxed_slice(),
                    groups: Vec::new().into_boxed_slice(),
                    marker: MarkerTree::TRUE,
                    source: RequirementSource::Registry {
                        specifier: VersionSpecifiers::empty(),
                        index: None,
                        conflict: None,
                    },
                    origin: None,
                })
                .collect(),
        )
    }
}
