use std::cmp::Ordering;
use std::collections::Bound;
use std::ops::Deref;

use itertools::Itertools;
use pubgrub::range::Range;

use distribution_filename::WheelFilename;
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
    /// For a workspace, it's the union of all `requires-python` values in the workspace. If no
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

    /// Returns a [`RequiresPython`] from a version specifier.
    pub fn from_specifiers(specifiers: &VersionSpecifiers) -> Result<Self, RequiresPythonError> {
        let bound = RequiresPythonBound(
            crate::pubgrub::PubGrubSpecifier::from_release_specifiers(specifiers)?
                .iter()
                .next()
                .map(|(lower, _)| lower.clone())
                .unwrap_or(Bound::Unbounded),
        );
        Ok(Self {
            specifiers: specifiers.clone(),
            bound,
        })
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

    /// Returns the [`RequiresPythonBound`] truncated to the major and minor version.
    pub fn bound_major_minor(&self) -> RequiresPythonBound {
        match self.bound.as_ref() {
            // Ex) `>=3.10.1` -> `>=3.10`
            Bound::Included(version) => RequiresPythonBound(Bound::Included(Version::new(
                version.release().iter().take(2),
            ))),
            // Ex) `>3.10.1` -> `>=3.10`
            // This is unintuitive, but `>3.10.1` does indicate that _some_ version of Python 3.10
            // is supported.
            Bound::Excluded(version) => RequiresPythonBound(Bound::Included(Version::new(
                version.release().iter().take(2),
            ))),
            Bound::Unbounded => RequiresPythonBound(Bound::Unbounded),
        }
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

    /// Returns `false` if the wheel's tags state it can't be used in the given Python version
    /// range.
    ///
    /// It is meant to filter out clearly unusable wheels with perfect specificity and acceptable
    /// sensitivity, we return `true` if the tags are unknown.
    pub fn matches_wheel_tag(&self, wheel: &WheelFilename) -> bool {
        wheel.abi_tag.iter().any(|abi_tag| {
            if abi_tag == "abi3" {
                // Universal tags are allowed.
                true
            } else if abi_tag == "none" {
                wheel.python_tag.iter().any(|python_tag| {
                    // Remove `py2-none-any` and `py27-none-any`.
                    if python_tag.starts_with("py2") {
                        return false;
                    }

                    // Remove (e.g.) `cp36-none-any` if the specifier is `==3.10.*`.
                    let Some(minor) = python_tag
                        .strip_prefix("cp3")
                        .or_else(|| python_tag.strip_prefix("pp3"))
                        .or_else(|| python_tag.strip_prefix("py3"))
                    else {
                        return true;
                    };
                    let Ok(minor) = minor.parse::<u64>() else {
                        return true;
                    };

                    // Ex) If the wheel bound is `3.6`, then it doesn't match `>=3.10`.
                    let wheel_bound =
                        RequiresPythonBound(Bound::Included(Version::new([3, minor])));
                    wheel_bound >= self.bound_major_minor()
                })
            } else if abi_tag.starts_with("cp2") || abi_tag.starts_with("pypy2") {
                // Python 2 is never allowed.
                false
            } else if let Some(minor_no_dot_abi) = abi_tag.strip_prefix("cp3") {
                // Remove ABI tags, both old (dmu) and future (t, and all other letters).
                let minor_not_dot = minor_no_dot_abi.trim_matches(char::is_alphabetic);
                let Ok(minor) = minor_not_dot.parse::<u64>() else {
                    // Unknown version pattern are allowed.
                    return true;
                };

                let wheel_bound = RequiresPythonBound(Bound::Included(Version::new([3, minor])));
                wheel_bound >= self.bound_major_minor()
            } else if let Some(minor_no_dot_abi) = abi_tag.strip_prefix("pypy3") {
                // Given  `pypy39_pp73`, we just removed `pypy3`, now we remove `_pp73` ...
                let Some((minor_not_dot, _)) = minor_no_dot_abi.split_once('_') else {
                    // Unknown version pattern are allowed.
                    return true;
                };
                // ... and get `9`.
                let Ok(minor) = minor_not_dot.parse::<u64>() else {
                    // Unknown version pattern are allowed.
                    return true;
                };

                let wheel_bound = RequiresPythonBound(Bound::Included(Version::new([3, minor])));
                wheel_bound >= self.bound_major_minor()
            } else {
                // Unknown python tag -> allowed.
                true
            }
        })
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
            (Bound::Included(a), Bound::Excluded(b)) => a.cmp(b).then(Ordering::Less),
            (Bound::Excluded(a), Bound::Included(b)) => a.cmp(b).then(Ordering::Greater),
            (Bound::Excluded(a), Bound::Excluded(b)) => a.cmp(b),
            (Bound::Unbounded, Bound::Unbounded) => Ordering::Equal,
            (Bound::Unbounded, _) => Ordering::Less,
            (_, Bound::Unbounded) => Ordering::Greater,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cmp::Ordering;
    use std::collections::Bound;
    use std::str::FromStr;

    use distribution_filename::WheelFilename;
    use pep440_rs::{Version, VersionSpecifiers};

    use crate::{RequiresPython, RequiresPythonBound};

    #[test]
    fn requires_python_included() {
        let version_specifiers = VersionSpecifiers::from_str("==3.10.*").unwrap();
        let requires_python = RequiresPython::union(std::iter::once(&version_specifiers))
            .unwrap()
            .unwrap();
        let wheel_names = &[
            "bcrypt-4.1.3-cp37-abi3-macosx_10_12_universal2.whl",
            "black-24.4.2-cp310-cp310-win_amd64.whl",
            "black-24.4.2-cp310-none-win_amd64.whl",
            "cbor2-5.6.4-py3-none-any.whl",
            "watchfiles-0.22.0-pp310-pypy310_pp73-macosx_11_0_arm64.whl",
            "dearpygui-1.11.1-cp312-cp312-win_amd64.whl",
        ];
        for wheel_name in wheel_names {
            assert!(
                requires_python.matches_wheel_tag(&WheelFilename::from_str(wheel_name).unwrap()),
                "{wheel_name}"
            );
        }

        let version_specifiers = VersionSpecifiers::from_str(">=3.12.3").unwrap();
        let requires_python = RequiresPython::union(std::iter::once(&version_specifiers))
            .unwrap()
            .unwrap();
        let wheel_names = &["dearpygui-1.11.1-cp312-cp312-win_amd64.whl"];
        for wheel_name in wheel_names {
            assert!(
                requires_python.matches_wheel_tag(&WheelFilename::from_str(wheel_name).unwrap()),
                "{wheel_name}"
            );
        }
    }

    #[test]
    fn requires_python_dropped() {
        let version_specifiers = VersionSpecifiers::from_str("==3.10.*").unwrap();
        let requires_python = RequiresPython::union(std::iter::once(&version_specifiers))
            .unwrap()
            .unwrap();
        let wheel_names = &[
            "PySocks-1.7.1-py27-none-any.whl",
            "black-24.4.2-cp39-cp39-win_amd64.whl",
            "psutil-6.0.0-cp36-cp36m-win32.whl",
            "pydantic_core-2.20.1-pp39-pypy39_pp73-win_amd64.whl",
            "torch-1.10.0-cp36-none-macosx_10_9_x86_64.whl",
            "torch-1.10.0-py36-none-macosx_10_9_x86_64.whl",
        ];
        for wheel_name in wheel_names {
            assert!(
                !requires_python.matches_wheel_tag(&WheelFilename::from_str(wheel_name).unwrap()),
                "{wheel_name}"
            );
        }

        let version_specifiers = VersionSpecifiers::from_str(">=3.12.3").unwrap();
        let requires_python = RequiresPython::union(std::iter::once(&version_specifiers))
            .unwrap()
            .unwrap();
        let wheel_names = &["dearpygui-1.11.1-cp310-cp310-win_amd64.whl"];
        for wheel_name in wheel_names {
            assert!(
                !requires_python.matches_wheel_tag(&WheelFilename::from_str(wheel_name).unwrap()),
                "{wheel_name}"
            );
        }
    }

    #[test]
    fn ordering() {
        let versions = &[
            // No bound
            RequiresPythonBound::new(Bound::Unbounded),
            // >=3.8
            RequiresPythonBound::new(Bound::Included(Version::new([3, 8]))),
            // >3.8
            RequiresPythonBound::new(Bound::Excluded(Version::new([3, 8]))),
            // >=3.8.1
            RequiresPythonBound::new(Bound::Included(Version::new([3, 8, 1]))),
            // >3.8.1
            RequiresPythonBound::new(Bound::Excluded(Version::new([3, 8, 1]))),
        ];
        for (i, v1) in versions.iter().enumerate() {
            for v2 in &versions[i + 1..] {
                assert_eq!(v1.cmp(v2), Ordering::Less, "less: {v1:?}\ngreater: {v2:?}",);
            }
        }
    }
}
