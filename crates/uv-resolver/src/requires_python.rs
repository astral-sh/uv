use std::collections::Bound;

use itertools::Itertools;
use pubgrub::range::Range;

use pep440_rs::{Version, VersionSpecifier, VersionSpecifiers};

#[derive(thiserror::Error, Debug)]
pub enum RequiresPythonError {
    #[error(transparent)]
    PubGrub(#[from] crate::pubgrub::PubGrubSpecifierError),
}

/// The `Requires-Python` requirement specifier.
///
/// We treat `Requires-Python` as a lower bound. For example, if the requirement expresses
/// `>=3.8, <4`, we treat it as `>=3.8`. `Requires-Python` itself was intended to enable
/// packages to drop support for older versions of Python without breaking installations on
/// those versions, and packages cannot know whether they are compatible with future, unreleased
/// versions of Python.
///
/// See: <https://packaging.python.org/en/latest/guides/dropping-older-python-versions/>
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct RequiresPython(VersionSpecifiers);

impl RequiresPython {
    /// Returns a [`RequiresPython`] to express `>=` equality with the given version.
    pub fn greater_than_equal_version(version: Version) -> Self {
        Self(VersionSpecifiers::from(
            VersionSpecifier::greater_than_equal_version(version),
        ))
    }

    /// Returns a [`RequiresPython`] to express the union of the given version specifiers.
    ///
    /// For example, given `>=3.8` and `>=3.9`, this would return `>=3.8`.
    pub fn union<'a>(
        specifiers: impl Iterator<Item = &'a VersionSpecifiers>,
    ) -> Result<Option<Self>, RequiresPythonError> {
        // Convert to PubGrub range and perform a union.
        let range = specifiers
            .into_iter()
            .map(crate::pubgrub::PubGrubSpecifier::try_from)
            .fold_ok(None, |range: Option<Range<Version>>, requires_python| {
                if let Some(range) = range {
                    Some(range.union(&requires_python.into()))
                } else {
                    Some(requires_python.into())
                }
            })?;

        let Some(range) = range else {
            return Ok(None);
        };

        // Convert back to PEP 440 specifiers.
        let requires_python = Self(
            range
                .iter()
                .flat_map(VersionSpecifier::from_bounds)
                .collect(),
        );

        Ok(Some(requires_python))
    }

    /// Returns `true` if the `Requires-Python` is compatible with the given version.
    pub fn contains(&self, version: &Version) -> bool {
        self.0.contains(version)
    }

    /// Returns `true` if the `Requires-Python` is compatible with the given version specifiers.
    ///
    /// For example, if the `Requires-Python` is `>=3.8`, then `>=3.7` would be considered
    /// compatible, since all versions in the `Requires-Python` range are also covered by the
    /// provided range. However, `>=3.9` would not be considered compatible, as the
    /// `Requires-Python` includes Python 3.8, but `>=3.9` does not.
    pub fn is_contained_by(&self, target: &VersionSpecifiers) -> bool {
        let Ok(requires_python) = crate::pubgrub::PubGrubSpecifier::try_from(&self.0) else {
            return false;
        };

        let Ok(target) = crate::pubgrub::PubGrubSpecifier::try_from(target) else {
            return false;
        };

        // If the dependency has no lower bound, then it supports all versions.
        let Some((target_lower, _)) = target.iter().next() else {
            return true;
        };

        // If we have no lower bound, then there must be versions we support that the
        // dependency does not.
        let Some((requires_python_lower, _)) = requires_python.iter().next() else {
            return false;
        };

        // We want, e.g., `requires_python_lower` to be `>=3.8` and `version_lower` to be
        // `>=3.7`.
        //
        // That is: `version_lower` should be less than or equal to `requires_python_lower`.
        match (target_lower, requires_python_lower) {
            (Bound::Included(target_lower), Bound::Included(requires_python_lower)) => {
                target_lower <= requires_python_lower
            }
            (Bound::Excluded(target_lower), Bound::Included(requires_python_lower)) => {
                target_lower < requires_python_lower
            }
            (Bound::Included(target_lower), Bound::Excluded(requires_python_lower)) => {
                target_lower <= requires_python_lower
            }
            (Bound::Excluded(target_lower), Bound::Excluded(requires_python_lower)) => {
                target_lower < requires_python_lower
            }
            // If the dependency has no lower bound, then it supports all versions.
            (Bound::Unbounded, _) => true,
            // If we have no lower bound, then there must be versions we support that the
            // dependency does not.
            (_, Bound::Unbounded) => false,
        }
    }

    /// Returns the [`VersionSpecifiers`] for the `Requires-Python` specifier.
    pub fn specifiers(&self) -> &VersionSpecifiers {
        &self.0
    }
}

impl std::fmt::Display for RequiresPython {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.0, f)
    }
}

impl serde::Serialize for RequiresPython {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

impl<'de> serde::Deserialize<'de> for RequiresPython {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let specifiers = VersionSpecifiers::deserialize(deserializer)?;
        Ok(Self(specifiers))
    }
}
