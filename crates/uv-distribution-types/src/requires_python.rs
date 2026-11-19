use std::collections::Bound;

use version_ranges::Ranges;

use uv_distribution_filename::WheelFilename;
use uv_pep440::{
    LowerBound, UpperBound, Version, VersionSpecifier, VersionSpecifiers,
    release_specifiers_to_ranges,
};
use uv_pep508::{MarkerExpression, MarkerTree, MarkerValueVersion};
use uv_platform_tags::{AbiTag, CPythonAbiVariants, LanguageTag};

/// The `Requires-Python` requirement specifier.
///
/// See: <https://packaging.python.org/en/latest/guides/dropping-older-python-versions/>
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct RequiresPython {
    /// The supported Python versions as provides by the user, usually through the `requires-python`
    /// field in `pyproject.toml`.
    ///
    /// For a workspace, it's the intersection of all `requires-python` values in the workspace. If
    /// no bound was provided by the user, it's greater equal the current Python version.
    ///
    /// The specifiers remain static over the lifetime of the workspace, such that they
    /// represent the initial Python version constraints.
    specifiers: VersionSpecifiers,
    /// The lower and upper bounds of the given specifiers.
    ///
    /// The range may be narrowed over the course of dependency resolution as the resolver
    /// investigates environments with stricter Python version constraints.
    range: RequiresPythonRange,
}

impl RequiresPython {
    /// Returns a [`RequiresPython`] to express `>=` equality with the given version.
    pub fn greater_than_equal_version(version: &Version) -> Self {
        let version = version.only_release();
        Self {
            specifiers: VersionSpecifiers::from(VersionSpecifier::greater_than_equal_version(
                version.clone(),
            )),
            range: RequiresPythonRange(
                LowerBound::new(Bound::Included(version.clone())),
                UpperBound::new(Bound::Unbounded),
            ),
        }
    }

    /// Returns a [`RequiresPython`] from a version specifier.
    pub fn from_specifiers(specifiers: VersionSpecifiers) -> Self {
        let (lower_bound, upper_bound) = release_specifiers_to_ranges(specifiers.clone())
            .bounding_range()
            .map(|(lower_bound, upper_bound)| (lower_bound.cloned(), upper_bound.cloned()))
            .unwrap_or((Bound::Unbounded, Bound::Unbounded));
        Self {
            specifiers,
            range: RequiresPythonRange(LowerBound::new(lower_bound), UpperBound::new(upper_bound)),
        }
    }

    /// Returns a [`RequiresPython`] to express the intersection of the given version specifiers.
    ///
    /// For example, given `>=3.8` and `>=3.9`, this would return `>=3.9`.
    pub fn intersection<'a>(
        specifiers: impl Iterator<Item = &'a VersionSpecifiers>,
    ) -> Option<Self> {
        // Convert to PubGrub range and perform an intersection.
        let range = specifiers
            .map(|specs| release_specifiers_to_ranges(specs.clone()))
            .reduce(|acc, r| acc.intersection(&r))?;

        // If the intersection is empty, return `None`.
        if range.is_empty() {
            return None;
        }

        // Convert back to PEP 440 specifiers.
        let specifiers = VersionSpecifiers::from_release_only_bounds(range.iter());

        // Extract the bounds.
        let range = RequiresPythonRange::from_range(&range);

        Some(Self { specifiers, range })
    }

    /// Split the [`RequiresPython`] at the given version.
    ///
    /// For example, if the current requirement is `>=3.10`, and the split point is `3.11`, then
    /// the result will be `>=3.10 and <3.11` and `>=3.11`.
    pub fn split(&self, bound: Bound<Version>) -> Option<(Self, Self)> {
        let RequiresPythonRange(.., upper) = &self.range;

        let upper = Ranges::from_range_bounds((bound, upper.clone().into()));
        let lower = upper.complement();

        // Intersect left and right with the existing range.
        let lower = lower.intersection(&Ranges::from(self.range.clone()));
        let upper = upper.intersection(&Ranges::from(self.range.clone()));

        if lower.is_empty() || upper.is_empty() {
            None
        } else {
            Some((
                Self {
                    specifiers: VersionSpecifiers::from_release_only_bounds(lower.iter()),
                    range: RequiresPythonRange::from_range(&lower),
                },
                Self {
                    specifiers: VersionSpecifiers::from_release_only_bounds(upper.iter()),
                    range: RequiresPythonRange::from_range(&upper),
                },
            ))
        }
    }

    /// Narrow the [`RequiresPython`] by computing the intersection with the given range.
    ///
    /// Returns `None` if the given range is not narrower than the current range.
    pub fn narrow(&self, range: &RequiresPythonRange) -> Option<Self> {
        if *range == self.range {
            return None;
        }
        let lower = if range.0 >= self.range.0 {
            Some(&range.0)
        } else {
            None
        };
        let upper = if range.1 <= self.range.1 {
            Some(&range.1)
        } else {
            None
        };
        let range = match (lower, upper) {
            (Some(lower), Some(upper)) => Some(RequiresPythonRange(lower.clone(), upper.clone())),
            (Some(lower), None) => Some(RequiresPythonRange(lower.clone(), self.range.1.clone())),
            (None, Some(upper)) => Some(RequiresPythonRange(self.range.0.clone(), upper.clone())),
            (None, None) => None,
        }?;
        Some(Self {
            specifiers: range.specifiers(),
            range,
        })
    }

    /// Returns this `Requires-Python` specifier as an equivalent
    /// [`MarkerTree`] utilizing the `python_full_version` marker field.
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
        match (self.range.0.as_ref(), self.range.1.as_ref()) {
            (Bound::Included(lower), Bound::Included(upper)) => {
                let mut lower = MarkerTree::expression(MarkerExpression::Version {
                    key: MarkerValueVersion::PythonFullVersion,
                    specifier: VersionSpecifier::greater_than_equal_version(lower.clone()),
                });
                let upper = MarkerTree::expression(MarkerExpression::Version {
                    key: MarkerValueVersion::PythonFullVersion,
                    specifier: VersionSpecifier::less_than_equal_version(upper.clone()),
                });
                lower = lower.and(upper);
                lower
            }
            (Bound::Included(lower), Bound::Excluded(upper)) => {
                let mut lower = MarkerTree::expression(MarkerExpression::Version {
                    key: MarkerValueVersion::PythonFullVersion,
                    specifier: VersionSpecifier::greater_than_equal_version(lower.clone()),
                });
                let upper = MarkerTree::expression(MarkerExpression::Version {
                    key: MarkerValueVersion::PythonFullVersion,
                    specifier: VersionSpecifier::less_than_version(upper.clone()),
                });
                lower = lower.and(upper);
                lower
            }
            (Bound::Excluded(lower), Bound::Included(upper)) => {
                let mut lower = MarkerTree::expression(MarkerExpression::Version {
                    key: MarkerValueVersion::PythonFullVersion,
                    specifier: VersionSpecifier::greater_than_version(lower.clone()),
                });
                let upper = MarkerTree::expression(MarkerExpression::Version {
                    key: MarkerValueVersion::PythonFullVersion,
                    specifier: VersionSpecifier::less_than_equal_version(upper.clone()),
                });
                lower = lower.and(upper);
                lower
            }
            (Bound::Excluded(lower), Bound::Excluded(upper)) => {
                let mut lower = MarkerTree::expression(MarkerExpression::Version {
                    key: MarkerValueVersion::PythonFullVersion,
                    specifier: VersionSpecifier::greater_than_version(lower.clone()),
                });
                let upper = MarkerTree::expression(MarkerExpression::Version {
                    key: MarkerValueVersion::PythonFullVersion,
                    specifier: VersionSpecifier::less_than_version(upper.clone()),
                });
                lower = lower.and(upper);
                lower
            }
            (Bound::Unbounded, Bound::Unbounded) => MarkerTree::TRUE,
            (Bound::Unbounded, Bound::Included(upper)) => {
                MarkerTree::expression(MarkerExpression::Version {
                    key: MarkerValueVersion::PythonFullVersion,
                    specifier: VersionSpecifier::less_than_equal_version(upper.clone()),
                })
            }
            (Bound::Unbounded, Bound::Excluded(upper)) => {
                MarkerTree::expression(MarkerExpression::Version {
                    key: MarkerValueVersion::PythonFullVersion,
                    specifier: VersionSpecifier::less_than_version(upper.clone()),
                })
            }
            (Bound::Included(lower), Bound::Unbounded) => {
                MarkerTree::expression(MarkerExpression::Version {
                    key: MarkerValueVersion::PythonFullVersion,
                    specifier: VersionSpecifier::greater_than_equal_version(lower.clone()),
                })
            }
            (Bound::Excluded(lower), Bound::Unbounded) => {
                MarkerTree::expression(MarkerExpression::Version {
                    key: MarkerValueVersion::PythonFullVersion,
                    specifier: VersionSpecifier::greater_than_version(lower.clone()),
                })
            }
        }
    }

    /// Returns `true` if the `Requires-Python` is compatible with the given version.
    ///
    /// N.B. This operation should primarily be used when evaluating compatibility of Python
    /// versions against the user's own project. For example, if the user defines a
    /// `requires-python` in a `pyproject.toml`, this operation could be used to determine whether
    /// a given Python interpreter is compatible with the user's project.
    pub fn contains(&self, version: &Version) -> bool {
        let version = version.only_release();
        self.specifiers.contains(&version)
    }

    /// Returns `true` if the `Requires-Python` is contained by the given version specifiers.
    ///
    /// In this context, we treat `Requires-Python` as a lower bound. For example, if the
    /// requirement expresses `>=3.8, <4`, we treat it as `>=3.8`. `Requires-Python` itself was
    /// intended to enable packages to drop support for older versions of Python without breaking
    /// installations on those versions, and packages cannot know whether they are compatible with
    /// future, unreleased versions of Python.
    ///
    /// The specifiers are considered to "contain" the `Requires-Python` if the specifiers are
    /// compatible with all versions in the `Requires-Python` range (i.e., have a _lower_ lower
    /// bound).
    ///
    /// For example, if the `Requires-Python` is `>=3.8`, then `>=3.7` would be considered
    /// compatible, since all versions in the `Requires-Python` range are also covered by the
    /// provided range. However, `>=3.9` would not be considered compatible, as the
    /// `Requires-Python` includes Python 3.8, but `>=3.9` does not.
    ///
    /// N.B. This operation should primarily be used when evaluating the compatibility of a
    /// project's `Requires-Python` specifier against a dependency's `Requires-Python` specifier.
    pub fn is_contained_by(&self, target: &VersionSpecifiers) -> bool {
        let target = release_specifiers_to_ranges(target.clone())
            .bounding_range()
            .map(|bounding_range| bounding_range.0.cloned())
            .unwrap_or(Bound::Unbounded);

        // We want, e.g., `self.range.lower()` to be `>=3.8` and `target` to be `>=3.7`.
        //
        // That is: `target` should be less than or equal to `self.range.lower()`.
        *self.range.lower() >= LowerBound(target.clone())
    }

    /// Returns the [`VersionSpecifiers`] for the `Requires-Python` specifier.
    pub fn specifiers(&self) -> &VersionSpecifiers {
        &self.specifiers
    }

    /// Returns `true` if the `Requires-Python` specifier is unbounded.
    pub fn is_unbounded(&self) -> bool {
        self.range.lower().as_ref() == Bound::Unbounded
    }

    /// Returns `true` if the `Requires-Python` specifier is set to an exact version
    /// without specifying a patch version. (e.g. `==3.10`)
    pub fn is_exact_without_patch(&self) -> bool {
        match self.range.lower().as_ref() {
            Bound::Included(version) => {
                version.release().len() == 2
                    && self.range.upper().as_ref() == Bound::Included(version)
            }
            _ => false,
        }
    }

    /// Returns the [`RequiresPythonRange`] bounding the `Requires-Python` specifier.
    pub fn range(&self) -> &RequiresPythonRange {
        &self.range
    }

    /// Returns a wheel tag that's compatible with the `Requires-Python` specifier.
    pub fn abi_tag(&self) -> Option<AbiTag> {
        match self.range.lower().as_ref() {
            Bound::Included(version) | Bound::Excluded(version) => {
                let major = version.release().first().copied()?;
                let major = u8::try_from(major).ok()?;
                let minor = version.release().get(1).copied()?;
                let minor = u8::try_from(minor).ok()?;
                Some(AbiTag::CPython {
                    variant: CPythonAbiVariants::default(),
                    python_version: (major, minor),
                })
            }
            Bound::Unbounded => None,
        }
    }

    /// Simplifies the given markers in such a way as to assume that
    /// the Python version is constrained by this Python version bound.
    ///
    /// For example, with `requires-python = '>=3.8'`, a marker like this:
    ///
    /// ```text
    /// python_full_version >= '3.8' and python_full_version < '3.12'
    /// ```
    ///
    /// Will be simplified to:
    ///
    /// ```text
    /// python_full_version < '3.12'
    /// ```
    ///
    /// That is, `python_full_version >= '3.8'` is assumed to be true by virtue
    /// of `requires-python`, and is thus not needed in the marker.
    ///
    /// This should be used in contexts in which this assumption is valid to
    /// make. Generally, this means it should not be used inside the resolver,
    /// but instead near the boundaries of the system (like formatting error
    /// messages and writing the lock file). The reason for this is that
    /// this simplification fundamentally changes the meaning of the marker,
    /// and the *only* correct way to interpret it is in a context in which
    /// `requires-python` is known to be true. For example, when markers from
    /// a lock file are deserialized and turned into a `ResolutionGraph`, the
    /// markers are "complexified" to put the `requires-python` assumption back
    /// into the marker explicitly.
    pub fn simplify_markers(&self, marker: MarkerTree) -> MarkerTree {
        let (lower, upper) = (self.range().lower(), self.range().upper());
        marker.simplify_python_versions(lower.as_ref(), upper.as_ref())
    }

    /// The inverse of `simplify_markers`.
    ///
    /// This should be applied near the boundaries of uv when markers are
    /// deserialized from a context where `requires-python` is assumed. For
    /// example, with `requires-python = '>=3.8'` and a marker like:
    ///
    /// ```text
    /// python_full_version < '3.12'
    /// ```
    ///
    /// It will be "complexified" to:
    ///
    /// ```text
    /// python_full_version >= '3.8' and python_full_version < '3.12'
    /// ```
    pub fn complexify_markers(&self, marker: MarkerTree) -> MarkerTree {
        let (lower, upper) = (self.range().lower(), self.range().upper());
        marker.complexify_python_versions(lower.as_ref(), upper.as_ref())
    }

    /// Check whether a wheel can match the Python version range for a lockfile consumer,
    /// which might recognize additional tags.
    pub fn may_match_wheel_tag(&self, wheel: &WheelFilename) -> bool {
        if self.matches_wheel_tag(wheel) {
            return true;
        }

        let python_tags = wheel.complete_python_tags();
        let matches_python = |stable_abi| {
            python_tags.as_deref().is_none_or(|tags| {
                tags.iter()
                    .any(|tag| self.matches_python_tag(*tag, stable_abi))
            })
        };
        let Some(abi_tags) = wheel.complete_abi_tags() else {
            // An unknown ABI may be stable, in which case cp311 supports CPython 3.11 and later.
            return matches_python(true);
        };
        abi_tags
            .iter()
            .any(|abi| self.matches_abi_tag(*abi) && matches_python(abi.is_stable_abi()))
    }

    /// Check whether a tag recognized by uv can match the Python version range.
    pub fn matches_wheel_tag(&self, wheel: &WheelFilename) -> bool {
        wheel.abi_tags().iter().any(|abi| {
            if *abi == AbiTag::None {
                wheel.python_tags().iter().any(|tag| {
                    matches!(tag, LanguageTag::CPythonMajor { .. })
                        || self.matches_python_tag(*tag, false)
                })
            } else {
                self.matches_abi_tag(*abi)
            }
        })
    }

    /// Check the Python tag's version, accounting for the CPython stable ABI.
    fn matches_python_tag(&self, tag: LanguageTag, stable_abi: bool) -> bool {
        match tag {
            LanguageTag::None => true,
            LanguageTag::Python { major, minor: None } | LanguageTag::CPythonMajor { major } => {
                major != 2
            }
            LanguageTag::Python {
                major,
                minor: Some(minor),
            } => self.matches_python_version((major, minor), false),
            LanguageTag::CPython { python_version } => {
                self.matches_python_version(python_version, !stable_abi)
            }
            LanguageTag::PyPy { python_version }
            | LanguageTag::GraalPy { python_version }
            | LanguageTag::Pyston { python_version } => {
                self.matches_python_version(python_version, true)
            }
        }
    }

    /// Check the Python version encoded by the ABI tag, if any.
    fn matches_abi_tag(&self, tag: AbiTag) -> bool {
        match tag {
            AbiTag::CPython { python_version, .. }
            | AbiTag::PyPy {
                python_version: Some(python_version),
                ..
            }
            | AbiTag::GraalPy { python_version, .. } => {
                self.matches_python_version(python_version, true)
            }
            AbiTag::PyPy {
                python_version: None,
                ..
            } => false,
            AbiTag::None | AbiTag::Abi3 | AbiTag::Abi3T | AbiTag::Pyston { .. } => true,
        }
    }

    /// A Python version in a wheel tag is either exact (`cp311-cp311`) or a minimum
    /// (`py311-none` or `cp311-abi3`).
    fn matches_python_version(&self, version: (u8, u8), exact: bool) -> bool {
        match version {
            (2, _) => false,
            (3, minor) => {
                let version = Version::new([3, u64::from(minor)]);
                (!exact || self.range.lower().major_minor().contains(&version))
                    && self.range.upper().major_minor().contains(&version)
            }
            _ => true,
        }
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
        let range = release_specifiers_to_ranges(specifiers.clone());
        let range = RequiresPythonRange::from_range(&range);
        Ok(Self { specifiers, range })
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct RequiresPythonRange(LowerBound, UpperBound);

impl RequiresPythonRange {
    /// Initialize a [`RequiresPythonRange`] from a set of version [`Ranges`].
    fn from_range(range: &Ranges<Version>) -> Self {
        let (lower, upper) = range
            .bounding_range()
            .map(|(lower_bound, upper_bound)| (lower_bound.cloned(), upper_bound.cloned()))
            .unwrap_or((Bound::Unbounded, Bound::Unbounded));
        Self(LowerBound(lower), UpperBound(upper))
    }

    /// Initialize a [`RequiresPythonRange`] with the given bounds.
    pub fn new(lower: LowerBound, upper: UpperBound) -> Self {
        Self(lower, upper)
    }

    /// Returns the lower bound.
    pub fn lower(&self) -> &LowerBound {
        &self.0
    }

    /// Returns the upper bound.
    pub fn upper(&self) -> &UpperBound {
        &self.1
    }

    /// Returns the [`VersionSpecifiers`] for the range.
    fn specifiers(&self) -> VersionSpecifiers {
        [self.0.specifier(), self.1.specifier()]
            .into_iter()
            .flatten()
            .collect()
    }
}

impl Default for RequiresPythonRange {
    fn default() -> Self {
        Self(LowerBound(Bound::Unbounded), UpperBound(Bound::Unbounded))
    }
}

impl From<RequiresPythonRange> for Ranges<Version> {
    fn from(value: RequiresPythonRange) -> Self {
        Self::from_range_bounds::<(Bound<Version>, Bound<Version>), _>((
            value.0.into(),
            value.1.into(),
        ))
    }
}

/// A simplified marker is just like a normal marker, except it has possibly
/// been simplified by `requires-python`.
///
/// A simplified marker should only exist in contexts where a `requires-python`
/// setting can be assumed. In order to get a "normal" marker out of
/// a simplified marker, one must re-contextualize it by adding the
/// `requires-python` constraint back to the marker.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, PartialOrd, Ord, serde::Deserialize)]
#[serde(transparent)]
pub struct SimplifiedMarkerTree(
    #[serde(deserialize_with = "MarkerTree::deserialize_environment")] MarkerTree,
);

impl SimplifiedMarkerTree {
    /// Simplifies the given markers by assuming the given `requires-python`
    /// bound is true.
    pub fn new(requires_python: &RequiresPython, marker: MarkerTree) -> Self {
        Self(requires_python.simplify_markers(marker))
    }

    /// Complexifies the given markers by adding the given `requires-python` as
    /// a constraint to these simplified markers.
    pub fn into_marker(self, requires_python: &RequiresPython) -> MarkerTree {
        requires_python.complexify_markers(self.0)
    }

    /// Attempts to convert this simplified marker to a string.
    ///
    /// This only returns `None` when the underlying marker is always true,
    /// i.e., it matches all possible marker environments.
    pub fn try_to_string(self) -> Option<String> {
        self.0.try_to_string()
    }

    /// Returns the underlying marker tree without re-complexifying them.
    pub fn as_simplified_marker_tree(self) -> MarkerTree {
        self.0
    }

    /// Combine this simplified marker with another via a conjunction.
    pub fn and(&mut self, marker: Self) {
        self.0 = self.0.and(marker.0);
    }
}

#[cfg(test)]
mod tests {
    use std::cmp::Ordering;
    use std::collections::Bound;
    use std::str::FromStr;

    use uv_distribution_filename::WheelFilename;
    use uv_pep440::{LowerBound, UpperBound, Version, VersionSpecifiers};

    use crate::RequiresPython;

    #[test]
    fn requires_python_included() {
        let version_specifiers = VersionSpecifiers::from_str("==3.10.*").unwrap();
        let requires_python = RequiresPython::from_specifiers(version_specifiers);
        let wheel_names = &[
            "bcrypt-4.1.3-cp37-abi3-macosx_10_12_universal2.whl",
            "black-24.4.2-cp310-cp310-win_amd64.whl",
            "black-24.4.2-cp310-none-win_amd64.whl",
            "cbor2-5.6.4-py3-none-any.whl",
            "solace_pubsubplus-1.8.0-py36-none-manylinux_2_12_x86_64.whl",
            "torch-1.10.0-py310-none-macosx_10_9_x86_64.whl",
            "torch-1.10.0-py37-none-macosx_10_9_x86_64.whl",
            "watchfiles-0.22.0-pp310-pypy310_pp73-macosx_11_0_arm64.whl",
        ];
        for wheel_name in wheel_names {
            assert!(
                requires_python.matches_wheel_tag(&WheelFilename::from_str(wheel_name).unwrap()),
                "{wheel_name}"
            );
        }

        let version_specifiers = VersionSpecifiers::from_str(">=3.12.3").unwrap();
        let requires_python = RequiresPython::from_specifiers(version_specifiers);
        let wheel_names = &["dearpygui-1.11.1-cp312-cp312-win_amd64.whl"];
        for wheel_name in wheel_names {
            assert!(
                requires_python.matches_wheel_tag(&WheelFilename::from_str(wheel_name).unwrap()),
                "{wheel_name}"
            );
        }

        let version_specifiers = VersionSpecifiers::from_str("==3.12.6").unwrap();
        let requires_python = RequiresPython::from_specifiers(version_specifiers);
        let wheel_names = &["lxml-5.3.0-cp312-cp312-musllinux_1_2_aarch64.whl"];
        for wheel_name in wheel_names {
            assert!(
                requires_python.matches_wheel_tag(&WheelFilename::from_str(wheel_name).unwrap()),
                "{wheel_name}"
            );
        }

        let version_specifiers = VersionSpecifiers::from_str("==3.12").unwrap();
        let requires_python = RequiresPython::from_specifiers(version_specifiers);
        let wheel_names = &["lxml-5.3.0-cp312-cp312-musllinux_1_2_x86_64.whl"];
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
        let requires_python = RequiresPython::from_specifiers(version_specifiers);
        let wheel_names = &[
            // Even a stable ABI requires at least the version in the CPython tag.
            "demo-1.0-cp311-cp311.ABI3-linux_x86_64.whl",
            "demo-1.0-cp311-cp311.unknown-linux_x86_64.whl",
            "PySocks-1.7.1-py27-none-any.whl",
            "black-24.4.2-cp39-cp39-win_amd64.whl",
            "dearpygui-1.11.1-cp312-cp312-win_amd64.whl",
            "psutil-6.0.0-cp27-none-win32.whl",
            "psutil-6.0.0-cp36-cp36m-win32.whl",
            "pydantic_core-2.20.1-pp39-pypy39_pp73-win_amd64.whl",
            "torch-1.10.0-cp311-none-macosx_10_9_x86_64.whl",
            "torch-1.10.0-cp36-none-macosx_10_9_x86_64.whl",
            "torch-1.10.0-py311-none-macosx_10_9_x86_64.whl",
        ];
        for wheel_name in wheel_names {
            let wheel = WheelFilename::from_str(wheel_name).unwrap();
            assert!(!requires_python.matches_wheel_tag(&wheel), "{wheel_name}");
            assert!(!requires_python.may_match_wheel_tag(&wheel), "{wheel_name}");
        }

        let version_specifiers = VersionSpecifiers::from_str(">=3.12.3").unwrap();
        let requires_python = RequiresPython::from_specifiers(version_specifiers);
        let wheel_names = &["dearpygui-1.11.1-cp310-cp310-win_amd64.whl"];
        for wheel_name in wheel_names {
            assert!(
                !requires_python.matches_wheel_tag(&WheelFilename::from_str(wheel_name).unwrap()),
                "{wheel_name}"
            );
        }
    }

    #[test]
    fn lower_bound_ordering() {
        let versions = &[
            // No bound
            LowerBound::new(Bound::Unbounded),
            // >=3.8
            LowerBound::new(Bound::Included(Version::new([3, 8]))),
            // >3.8
            LowerBound::new(Bound::Excluded(Version::new([3, 8]))),
            // >=3.8.1
            LowerBound::new(Bound::Included(Version::new([3, 8, 1]))),
            // >3.8.1
            LowerBound::new(Bound::Excluded(Version::new([3, 8, 1]))),
        ];
        for (i, v1) in versions.iter().enumerate() {
            for v2 in &versions[i + 1..] {
                assert_eq!(v1.cmp(v2), Ordering::Less, "less: {v1:?}\ngreater: {v2:?}");
            }
        }
    }

    #[test]
    fn upper_bound_ordering() {
        let versions = &[
            // <3.8
            UpperBound::new(Bound::Excluded(Version::new([3, 8]))),
            // <=3.8
            UpperBound::new(Bound::Included(Version::new([3, 8]))),
            // <3.8.1
            UpperBound::new(Bound::Excluded(Version::new([3, 8, 1]))),
            // <=3.8.1
            UpperBound::new(Bound::Included(Version::new([3, 8, 1]))),
            // No bound
            UpperBound::new(Bound::Unbounded),
        ];
        for (i, v1) in versions.iter().enumerate() {
            for v2 in &versions[i + 1..] {
                assert_eq!(v1.cmp(v2), Ordering::Less, "less: {v1:?}\ngreater: {v2:?}");
            }
        }
    }

    #[test]
    fn is_exact_without_patch() {
        let test_cases = [
            ("==3.12", true),
            ("==3.10, <3.11", true),
            ("==3.10, <=3.11", true),
            ("==3.12.1", false),
            ("==3.12.*", false),
            ("==3.*", false),
            (">=3.10", false),
            (">3.9", false),
            ("<4.0", false),
            (">=3.10, <3.11", false),
            ("", false),
        ];
        for (version, expected) in test_cases {
            let version_specifiers = VersionSpecifiers::from_str(version).unwrap();
            let requires_python = RequiresPython::from_specifiers(version_specifiers);
            assert_eq!(requires_python.is_exact_without_patch(), expected);
        }
    }

    #[test]
    fn split_version() {
        // Splitting `>=3.10` on `>3.12` should result in `>=3.10, <=3.12` and `>3.12`.
        let version_specifiers = VersionSpecifiers::from_str(">=3.10").unwrap();
        let requires_python = RequiresPython::from_specifiers(version_specifiers);
        let (lower, upper) = requires_python
            .split(Bound::Excluded(Version::new([3, 12])))
            .unwrap();
        assert_eq!(
            lower,
            RequiresPython::from_specifiers(VersionSpecifiers::from_str(">=3.10, <=3.12").unwrap())
        );
        assert_eq!(
            upper,
            RequiresPython::from_specifiers(VersionSpecifiers::from_str(">3.12").unwrap())
        );

        // Splitting `>=3.10` on `>=3.12` should result in `>=3.10, <3.12` and `>=3.12`.
        let version_specifiers = VersionSpecifiers::from_str(">=3.10").unwrap();
        let requires_python = RequiresPython::from_specifiers(version_specifiers);
        let (lower, upper) = requires_python
            .split(Bound::Included(Version::new([3, 12])))
            .unwrap();
        assert_eq!(
            lower,
            RequiresPython::from_specifiers(VersionSpecifiers::from_str(">=3.10, <3.12").unwrap())
        );
        assert_eq!(
            upper,
            RequiresPython::from_specifiers(VersionSpecifiers::from_str(">=3.12").unwrap())
        );

        // Splitting `>=3.10` on `>=3.9` should return `None`.
        let version_specifiers = VersionSpecifiers::from_str(">=3.10").unwrap();
        let requires_python = RequiresPython::from_specifiers(version_specifiers);
        assert!(
            requires_python
                .split(Bound::Included(Version::new([3, 9])))
                .is_none()
        );

        // Splitting `>=3.10` on `>=3.10` should return `None`.
        let version_specifiers = VersionSpecifiers::from_str(">=3.10").unwrap();
        let requires_python = RequiresPython::from_specifiers(version_specifiers);
        assert!(
            requires_python
                .split(Bound::Included(Version::new([3, 10])))
                .is_none()
        );

        // Splitting `>=3.9, <3.13` on `>=3.11` should result in `>=3.9, <3.11` and `>=3.11, <3.13`.
        let version_specifiers = VersionSpecifiers::from_str(">=3.9, <3.13").unwrap();
        let requires_python = RequiresPython::from_specifiers(version_specifiers);
        let (lower, upper) = requires_python
            .split(Bound::Included(Version::new([3, 11])))
            .unwrap();
        assert_eq!(
            lower,
            RequiresPython::from_specifiers(VersionSpecifiers::from_str(">=3.9, <3.11").unwrap())
        );
        assert_eq!(
            upper,
            RequiresPython::from_specifiers(VersionSpecifiers::from_str(">=3.11, <3.13").unwrap())
        );
    }
}
