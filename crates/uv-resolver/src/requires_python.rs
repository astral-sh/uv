use std::collections::Bound;

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
    specifiers: VersionSpecifiers,
    bound: Bound<Version>,
}

impl RequiresPython {
    /// Returns a [`RequiresPython`] to express `>=` equality with the given version.
    pub fn greater_than_equal_version(version: Version) -> Self {
        Self {
            specifiers: VersionSpecifiers::from(VersionSpecifier::greater_than_equal_version(
                version.clone(),
            )),
            bound: Bound::Included(version),
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

        // Extract the lower bound.
        let bound = range
            .iter()
            .next()
            .map(|(lower, _)| lower.clone())
            .unwrap_or(Bound::Unbounded);

        // Convert back to PEP 440 specifiers.
        let specifiers = range
            .iter()
            .flat_map(VersionSpecifier::from_bounds)
            .collect();

        Ok(Some(Self { specifiers, bound }))
    }

    /// Returns `true` if the `Requires-Python` is compatible with the given version.
    pub fn contains(&self, version: &Version) -> bool {
        self.specifiers.contains(version)
    }

    /// Returns `true` if the `Requires-Python` is compatible with the given version specifiers.
    ///
    /// For example, if the `Requires-Python` is `>=3.8`, then `>=3.7` would be considered
    /// compatible, since all versions in the `Requires-Python` range are also covered by the
    /// provided range. However, `>=3.9` would not be considered compatible, as the
    /// `Requires-Python` includes Python 3.8, but `>=3.9` does not.
    pub fn is_contained_by(&self, target: &VersionSpecifiers) -> bool {
        let Ok(target) = crate::pubgrub::PubGrubSpecifier::try_from(target) else {
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
        //
        // When comparing, we also limit the comparison to the release segment, ignoring
        // pre-releases and such. This may or may not be correct.
        //
        // Imagine `target_lower` is `3.13.0b1`, and `requires_python_lower` is `3.13`.
        // That would be fine, since we're saying we support `3.13.0` and later, and `target_lower`
        // supports more than that.
        //
        // Next, imagine `requires_python_lower` is `3.13.0b1`, and `target_lower` is `3.13`.
        // Technically, that would _not_ be fine, since we're saying we support `3.13.0b1` and
        // later, but `target_lower` does not support that. For example, `target_lower` does not
        // support `3.13.0b1`, `3.13.0rc1`, etc.
        //
        // In practice, this is most relevant for cases like: `requires_python = "==3.8.*"`, with
        // `target = ">=3.8"`. In this case, `requires_python_lower` is actually `3.8.0.dev0`,
        // because `==3.8.*` allows development and pre-release versions. So there are versions we
        // want to support that aren't explicitly supported by `target`, which does _not_ include
        // pre-releases.
        //
        // Since this is a fairly common `Requires-Python` specifier, we handle it pragmatically
        // by only enforcing Python compatibility at the patch-release level.
        //
        // There are some potentially-bad outcomes here. For example, maybe the user _did_ request
        // `>=3.13.0b1`. In that case, maybe we _shouldn't_ allow resolution that only support
        // `3.13.0` and later, because we're saying we support the beta releases, but the dependency
        // does not. But, it's debatable.
        //
        // If this scheme proves problematic, we could explore using different semantics when
        // converting to PubGrub. For example, we could parse `==3.8.*` as `>=3.8,<3.9`. But this
        // too could be problematic. Imagine that the user requests `>=3.8.0b0`, and the target
        // declares `==3.8.*`. In this case, we _do_ want to allow resolution, because the target
        // is saying it supports all versions of `3.8`, including pre-releases. But under those
        // modified parsing semantics, we would fail. (You could argue, though, that users declaring
        // `==3.8.*` are not intending to support pre-releases, and so failing there is fine, but
        // it's also incorrect in its own way.)
        //
        // Alternatively, we could vary the semantics depending on whether or not the user included
        // a pre-release in their specifier, enforcing pre-release compatibility only if the user
        // explicitly requested it.
        match (target, &self.bound) {
            (Bound::Included(target_lower), Bound::Included(requires_python_lower)) => {
                target_lower.release() <= requires_python_lower.release()
            }
            (Bound::Excluded(target_lower), Bound::Included(requires_python_lower)) => {
                target_lower.release() < requires_python_lower.release()
            }
            (Bound::Included(target_lower), Bound::Excluded(requires_python_lower)) => {
                target_lower.release() <= requires_python_lower.release()
            }
            (Bound::Excluded(target_lower), Bound::Excluded(requires_python_lower)) => {
                target_lower.release() < requires_python_lower.release()
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

    /// Returns the lower [`Bound`] for the `Requires-Python` specifier.
    pub fn bound(&self) -> &Bound<Version> {
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
        let (op, version) = match self.bound {
            // If we see this anywhere, then it implies the marker
            // tree we would generate would always evaluate to
            // `true` because every possible Python version would
            // satisfy it.
            Bound::Unbounded => return MarkerTree::And(vec![]),
            Bound::Excluded(ref version) => {
                (Operator::GreaterThan, version.clone().without_local())
            }
            Bound::Included(ref version) => {
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
        let bound = crate::pubgrub::PubGrubSpecifier::try_from(&specifiers)
            .map_err(serde::de::Error::custom)?
            .iter()
            .next()
            .map(|(lower, _)| lower.clone())
            .unwrap_or(Bound::Unbounded);
        Ok(Self { specifiers, bound })
    }
}
