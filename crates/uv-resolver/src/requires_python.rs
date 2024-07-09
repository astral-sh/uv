use std::cmp::Ordering;
use std::collections::Bound;
use std::ops::Deref;

use itertools::Itertools;
use pubgrub::range::Range;

use pep440_rs::{Operator, Version, VersionSpecifier, VersionSpecifiers};
use pep508_rs::{MarkerExpression, MarkerTree, MarkerValueVersion};

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
pub struct RequiresPython {
    /// The supported Python versions as provides by the user, usually through the `requires-python`
    /// field in `pyproject.toml`.
    ///
    /// For a workspace, it's the union of all `requires-python` fields in the workspace. If no
    /// bound was provided by the user, it's greater equal the current Python version.
    specifiers: VersionSpecifiers,
    /// The lower bound from the `specifiers` field, i.e. greater or greater equal the lowest
    /// version allowed by `specifiers`.
    bound: RequiresPythonBound,
}

impl RequiresPython {
    /// Returns a [`RequiresPython`] to express `>=` equality with the given version.
    pub fn greater_than_equal_version(version: &Version) -> Self {
        let version = version.only_release();
        Self {
            specifiers: VersionSpecifiers::from(VersionSpecifier::greater_than_equal_version(
                version.clone(),
            )),
            bound: RequiresPythonBound(Bound::Included(version)),
        }
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
            .map(crate::pubgrub::PubGrubSpecifier::from_release_specifiers)
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

        // Extract the lower bound.
        let bound = RequiresPythonBound(
            range
                .iter()
                .next()
                .map(|(lower, _)| lower.clone())
                .unwrap_or(Bound::Unbounded),
        );

        // Convert back to PEP 440 specifiers.
        let specifiers = range
            .iter()
            .flat_map(VersionSpecifier::from_bounds)
            .collect();

        Ok(Some(Self { specifiers, bound }))
    }

    /// Narrow the [`RequiresPython`] to the given version, if it's stricter (i.e., greater) than
    /// the current target.
    pub fn narrow(&self, target: &RequiresPythonBound) -> Option<Self> {
        let target = VersionSpecifiers::from(VersionSpecifier::from_lower_bound(target)?);
        Self::union(std::iter::once(&target))
            .ok()
            .flatten()
            .filter(|next| next.bound > self.bound)
    }

    /// Returns `true` if the `Requires-Python` is compatible with the given version.
    pub fn contains(&self, version: &Version) -> bool {
        let version = version.only_release();
        self.specifiers.contains(&version)
    }

    /// Returns `true` if the `Requires-Python` is compatible with the given version specifiers.
    ///
    /// For example, if the `Requires-Python` is `>=3.8`, then `>=3.7` would be considered
    /// compatible, since all versions in the `Requires-Python` range are also covered by the
    /// provided range. However, `>=3.9` would not be considered compatible, as the
    /// `Requires-Python` includes Python 3.8, but `>=3.9` does not.
    pub fn is_contained_by(&self, target: &VersionSpecifiers) -> bool {
        let Ok(target) = crate::pubgrub::PubGrubSpecifier::from_release_specifiers(target) else {
            return false;
        };
        let target = target
            .iter()
            .next()
            .map(|(lower, _)| lower)
            .unwrap_or(&Bound::Unbounded);

        // We want, e.g., `requires_python_lower` to be `>=3.8` and `version_lower` to be
        // `>=3.7`.
        //
        // That is: `version_lower` should be less than or equal to `requires_python_lower`.
        match (target, self.bound.as_ref()) {
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
        &self.specifiers
    }

    /// Returns `true` if the `Requires-Python` specifier is unbounded.
    pub fn is_unbounded(&self) -> bool {
        self.bound.as_ref() == Bound::Unbounded
    }

    /// Returns the [`RequiresPythonBound`] for the `Requires-Python` specifier.
    pub fn bound(&self) -> &RequiresPythonBound {
        &self.bound
    }

    /// Returns this `Requires-Python` specifier as an equivalent marker
    /// expression utilizing the `python_version` marker field.
    ///
    /// This is useful for comparing a `Requires-Python` specifier with
    /// arbitrary marker expressions. For example, one can ask whether the
    /// returned marker expression is disjoint with another marker expression.
    /// If it is, then one can conclude that the `Requires-Python` specifier
    /// excludes the dependency with that other marker expression.
    ///
    /// If this `Requires-Python` specifier has no constraints, then this
    /// returns a marker tree that evaluates to `true` for all possible marker
    /// environments.
    pub fn to_marker_tree(&self) -> MarkerTree {
        let (op, version) = match self.bound.as_ref() {
            // If we see this anywhere, then it implies the marker
            // tree we would generate would always evaluate to
            // `true` because every possible Python version would
            // satisfy it.
            Bound::Unbounded => return MarkerTree::And(vec![]),
            Bound::Excluded(version) => (Operator::GreaterThan, version.clone().without_local()),
            Bound::Included(version) => {
                (Operator::GreaterThanEqual, version.clone().without_local())
            }
        };
        // For the `python_version` marker expression, it specifically only
        // supports truncate major/minor versions of Python. This means that
        // a `Requires-Python: 3.10.1` is satisfied by `python_version ==
        // '3.10'`. So for disjointness checking, we need to ensure that the
        // marker expression we generate for `Requires-Python` doesn't try to
        // be overly selective about the patch version. We do this by keeping
        // this part of our marker limited to the major and minor version
        // components only.
        let version_major_minor_only = Version::new(version.release().iter().take(2));
        let expr_python_version = MarkerExpression::Version {
            key: MarkerValueVersion::PythonVersion,
            // OK because a version specifier is only invalid when the
            // version is local (which is impossible here because we
            // strip it above) or if the operator is ~= (which is also
            // impossible here).
            specifier: VersionSpecifier::from_version(op, version_major_minor_only).unwrap(),
        };
        let expr_python_full_version = MarkerExpression::Version {
            key: MarkerValueVersion::PythonFullVersion,
            // For `python_full_version`, we can use the entire
            // version as-is.
            //
            // OK because a version specifier is only invalid when the
            // version is local (which is impossible here because we
            // strip it above) or if the operator is ~= (which is also
            // impossible here).
            specifier: VersionSpecifier::from_version(op, version).unwrap(),
        };
        MarkerTree::And(vec![
            MarkerTree::Expression(expr_python_version),
            MarkerTree::Expression(expr_python_full_version),
        ])
    }
}

impl std::fmt::Display for RequiresPython {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.specifiers, f)
    }
}

impl serde::Serialize for RequiresPython {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.specifiers.serialize(serializer)
    }
}

impl<'de> serde::Deserialize<'de> for RequiresPython {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let specifiers = VersionSpecifiers::deserialize(deserializer)?;
        let bound = RequiresPythonBound(
            crate::pubgrub::PubGrubSpecifier::from_release_specifiers(&specifiers)
                .map_err(serde::de::Error::custom)?
                .iter()
                .next()
                .map(|(lower, _)| lower.clone())
                .unwrap_or(Bound::Unbounded),
        );
        Ok(Self { specifiers, bound })
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct RequiresPythonBound(Bound<Version>);

impl RequiresPythonBound {
    pub fn new(bound: Bound<Version>) -> Self {
        Self(match bound {
            Bound::Included(version) => Bound::Included(version.only_release()),
            Bound::Excluded(version) => Bound::Excluded(version.only_release()),
            Bound::Unbounded => Bound::Unbounded,
        })
    }
}

impl From<RequiresPythonBound> for Range<Version> {
    fn from(value: RequiresPythonBound) -> Self {
        match value.0 {
            Bound::Included(version) => Range::higher_than(version),
            Bound::Excluded(version) => Range::strictly_higher_than(version),
            Bound::Unbounded => Range::full(),
        }
    }
}

impl Deref for RequiresPythonBound {
    type Target = Bound<Version>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl PartialOrd for RequiresPythonBound {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for RequiresPythonBound {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self.as_ref(), other.as_ref()) {
            (Bound::Included(a), Bound::Included(b)) => a.cmp(b),
            (Bound::Included(_), Bound::Excluded(_)) => Ordering::Less,
            (Bound::Excluded(_), Bound::Included(_)) => Ordering::Greater,
            (Bound::Excluded(a), Bound::Excluded(b)) => a.cmp(b),
            (Bound::Unbounded, _) => Ordering::Less,
            (_, Bound::Unbounded) => Ordering::Greater,
        }
    }
}
