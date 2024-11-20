use pubgrub::Range;
use std::cmp::Ordering;
use std::collections::Bound;
use std::ops::Deref;

use uv_distribution_filename::WheelFilename;
use uv_pep440::{release_specifiers_to_ranges, Version, VersionSpecifier, VersionSpecifiers};
use uv_pep508::{MarkerExpression, MarkerTree, MarkerValueVersion};

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
    pub fn from_specifiers(specifiers: &VersionSpecifiers) -> Self {
        let (lower_bound, upper_bound) = release_specifiers_to_ranges(specifiers.clone())
            .bounding_range()
            .map(|(lower_bound, upper_bound)| (lower_bound.cloned(), upper_bound.cloned()))
            .unwrap_or((Bound::Unbounded, Bound::Unbounded));
        Self {
            specifiers: specifiers.clone(),
            range: RequiresPythonRange(LowerBound(lower_bound), UpperBound(upper_bound)),
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
            .into_iter()
            .map(|specifier| release_specifiers_to_ranges(specifier.clone()))
            .fold(None, |range: Option<Range<Version>>, requires_python| {
                if let Some(range) = range {
                    Some(range.intersection(&requires_python))
                } else {
                    Some(requires_python)
                }
            })?;

        // Extract the bounds.
        let (lower_bound, upper_bound) = range
            .bounding_range()
            .map(|(lower_bound, upper_bound)| {
                (
                    LowerBound(lower_bound.cloned()),
                    UpperBound(upper_bound.cloned()),
                )
            })
            .unwrap_or((LowerBound::default(), UpperBound::default()));

        // Convert back to PEP 440 specifiers.
        let specifiers = VersionSpecifiers::from_release_only_bounds(range.iter());

        Some(Self {
            specifiers,
            range: RequiresPythonRange(lower_bound, upper_bound),
        })
    }

    /// Narrow the [`RequiresPython`] by computing the intersection with the given range.
    pub fn narrow(&self, range: &RequiresPythonRange) -> Option<Self> {
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
        // TODO(charlie): Consider re-computing the specifiers (or removing them entirely in favor
        // of tracking the range). After narrowing, the specifiers and range may be out of sync.
        match (lower, upper) {
            (Some(lower), Some(upper)) => Some(Self {
                specifiers: self.specifiers.clone(),
                range: RequiresPythonRange(lower.clone(), upper.clone()),
            }),
            (Some(lower), None) => Some(Self {
                specifiers: self.specifiers.clone(),
                range: RequiresPythonRange(lower.clone(), self.range.1.clone()),
            }),
            (None, Some(upper)) => Some(Self {
                specifiers: self.specifiers.clone(),
                range: RequiresPythonRange(self.range.0.clone(), upper.clone()),
            }),
            (None, None) => None,
        }
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
                lower.and(upper);
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
                lower.and(upper);
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
                lower.and(upper);
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
                lower.and(upper);
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

    /// Returns the [`RequiresPythonBound`] truncated to the major and minor version.
    pub fn bound_major_minor(&self) -> LowerBound {
        match self.range.lower().as_ref() {
            // Ex) `>=3.10.1` -> `>=3.10`
            Bound::Included(version) => LowerBound(Bound::Included(Version::new(
                version.release().iter().take(2),
            ))),
            // Ex) `>3.10.1` -> `>=3.10`
            // This is unintuitive, but `>3.10.1` does indicate that _some_ version of Python 3.10
            // is supported.
            Bound::Excluded(version) => LowerBound(Bound::Included(Version::new(
                version.release().iter().take(2),
            ))),
            Bound::Unbounded => LowerBound(Bound::Unbounded),
        }
    }

    /// Returns the [`Range`] bounding the `Requires-Python` specifier.
    pub fn range(&self) -> &RequiresPythonRange {
        &self.range
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
    pub(crate) fn simplify_markers(&self, marker: MarkerTree) -> MarkerTree {
        let (lower, upper) = (self.range().lower(), self.range().upper());
        marker.simplify_python_versions(lower.0.as_ref(), upper.0.as_ref())
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
    pub(crate) fn complexify_markers(&self, marker: MarkerTree) -> MarkerTree {
        let (lower, upper) = (self.range().lower(), self.range().upper());
        marker.complexify_python_versions(lower.0.as_ref(), upper.0.as_ref())
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
                    // Remove `py2-none-any` and `py27-none-any` and analogous `cp` and `pp` tags.
                    if python_tag.starts_with("py2")
                        || python_tag.starts_with("cp2")
                        || python_tag.starts_with("pp2")
                    {
                        return false;
                    }

                    // Remove (e.g.) `py312-none-any` if the specifier is `==3.10.*`. However,
                    // `py37-none-any` would be fine, since the `3.7` represents a lower bound.
                    if let Some(minor) = python_tag.strip_prefix("py3") {
                        let Ok(minor) = minor.parse::<u64>() else {
                            return true;
                        };

                        // Ex) If the wheel bound is `3.12`, then it doesn't match `<=3.10.`.
                        let wheel_bound = UpperBound(Bound::Included(Version::new([3, minor])));
                        if wheel_bound > self.range.upper().major_minor() {
                            return false;
                        }

                        return true;
                    };

                    // Remove (e.g.) `cp36-none-any` or `cp312-none-any` if the specifier is
                    // `==3.10.*`, since these tags require an exact match.
                    if let Some(minor) = python_tag
                        .strip_prefix("cp3")
                        .or_else(|| python_tag.strip_prefix("pp3"))
                    {
                        let Ok(minor) = minor.parse::<u64>() else {
                            return true;
                        };

                        // Ex) If the wheel bound is `3.6`, then it doesn't match `>=3.10`.
                        let wheel_bound = LowerBound(Bound::Included(Version::new([3, minor])));
                        if wheel_bound < self.range.lower().major_minor() {
                            return false;
                        }

                        // Ex) If the wheel bound is `3.12`, then it doesn't match `<=3.10.`.
                        let wheel_bound = UpperBound(Bound::Included(Version::new([3, minor])));
                        if wheel_bound > self.range.upper().major_minor() {
                            return false;
                        }

                        return true;
                    }

                    // Unknown tags are allowed.
                    true
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

                // Ex) If the wheel bound is `3.6`, then it doesn't match `>=3.10`.
                let wheel_bound = LowerBound(Bound::Included(Version::new([3, minor])));
                if wheel_bound < self.range.lower().major_minor() {
                    return false;
                }

                // Ex) If the wheel bound is `3.12`, then it doesn't match `<=3.10.`.
                let wheel_bound = UpperBound(Bound::Included(Version::new([3, minor])));
                if wheel_bound > self.range.upper().major_minor() {
                    return false;
                }

                true
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

                // Ex) If the wheel bound is `3.6`, then it doesn't match `>=3.10`.
                let wheel_bound = LowerBound(Bound::Included(Version::new([3, minor])));
                if wheel_bound < self.range.lower().major_minor() {
                    return false;
                }

                // Ex) If the wheel bound is `3.12`, then it doesn't match `<=3.10.`.
                let wheel_bound = UpperBound(Bound::Included(Version::new([3, minor])));
                if wheel_bound > self.range.upper().major_minor() {
                    return false;
                }

                true
            } else {
                // Unknown tags are allowed.
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
        let (lower_bound, upper_bound) = release_specifiers_to_ranges(specifiers.clone())
            .bounding_range()
            .map(|(lower_bound, upper_bound)| (lower_bound.cloned(), upper_bound.cloned()))
            .unwrap_or((Bound::Unbounded, Bound::Unbounded));
        Ok(Self {
            specifiers,
            range: RequiresPythonRange(LowerBound(lower_bound), UpperBound(upper_bound)),
        })
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct RequiresPythonRange(LowerBound, UpperBound);

impl RequiresPythonRange {
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
}

impl Default for RequiresPythonRange {
    fn default() -> Self {
        Self(LowerBound(Bound::Unbounded), UpperBound(Bound::Unbounded))
    }
}

impl From<RequiresPythonRange> for Range<Version> {
    fn from(value: RequiresPythonRange) -> Self {
        Range::from_range_bounds::<(Bound<Version>, Bound<Version>), _>((
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
#[derive(Clone, Debug, Default, Eq, PartialEq, PartialOrd, Ord, serde::Deserialize)]
pub(crate) struct SimplifiedMarkerTree(MarkerTree);

impl SimplifiedMarkerTree {
    /// Simplifies the given markers by assuming the given `requires-python`
    /// bound is true.
    pub(crate) fn new(
        requires_python: &RequiresPython,
        marker: MarkerTree,
    ) -> SimplifiedMarkerTree {
        SimplifiedMarkerTree(requires_python.simplify_markers(marker))
    }

    /// Complexifies the given markers by adding the given `requires-python` as
    /// a constraint to these simplified markers.
    pub(crate) fn into_marker(self, requires_python: &RequiresPython) -> MarkerTree {
        requires_python.complexify_markers(self.0)
    }

    /// Attempts to convert this simplified marker to a string.
    ///
    /// This only returns `None` when the underlying marker is always true,
    /// i.e., it matches all possible marker environments.
    pub(crate) fn try_to_string(&self) -> Option<String> {
        self.0.try_to_string()
    }

    /// Returns the underlying marker tree without re-complexifying them.
    pub(crate) fn as_simplified_marker_tree(&self) -> &MarkerTree {
        &self.0
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct LowerBound(Bound<Version>);

impl LowerBound {
    /// Return the [`LowerBound`] truncated to the major and minor version.
    fn major_minor(&self) -> Self {
        match &self.0 {
            // Ex) `>=3.10.1` -> `>=3.10`
            Bound::Included(version) => Self(Bound::Included(Version::new(
                version.release().iter().take(2),
            ))),
            // Ex) `>3.10.1` -> `>=3.10`.
            Bound::Excluded(version) => Self(Bound::Included(Version::new(
                version.release().iter().take(2),
            ))),
            Bound::Unbounded => Self(Bound::Unbounded),
        }
    }
}

impl PartialOrd for LowerBound {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// See: <https://github.com/pubgrub-rs/pubgrub/blob/4b4b44481c5f93f3233221dc736dd23e67e00992/src/range.rs#L324>
impl Ord for LowerBound {
    fn cmp(&self, other: &Self) -> Ordering {
        let left = self.0.as_ref();
        let right = other.0.as_ref();

        match (left, right) {
            // left:   ∞-----
            // right:  ∞-----
            (Bound::Unbounded, Bound::Unbounded) => Ordering::Equal,
            // left:     [---
            // right:  ∞-----
            (Bound::Included(_left), Bound::Unbounded) => Ordering::Greater,
            // left:     ]---
            // right:  ∞-----
            (Bound::Excluded(_left), Bound::Unbounded) => Ordering::Greater,
            // left:   ∞-----
            // right:    [---
            (Bound::Unbounded, Bound::Included(_right)) => Ordering::Less,
            // left:   [----- OR [----- OR   [-----
            // right:    [--- OR [----- OR [---
            (Bound::Included(left), Bound::Included(right)) => left.cmp(right),
            (Bound::Excluded(left), Bound::Included(right)) => match left.cmp(right) {
                // left:   ]-----
                // right:    [---
                Ordering::Less => Ordering::Less,
                // left:   ]-----
                // right:  [---
                Ordering::Equal => Ordering::Greater,
                // left:     ]---
                // right:  [-----
                Ordering::Greater => Ordering::Greater,
            },
            // left:   ∞-----
            // right:    ]---
            (Bound::Unbounded, Bound::Excluded(_right)) => Ordering::Less,
            (Bound::Included(left), Bound::Excluded(right)) => match left.cmp(right) {
                // left:   [-----
                // right:    ]---
                Ordering::Less => Ordering::Less,
                // left:   [-----
                // right:  ]---
                Ordering::Equal => Ordering::Less,
                // left:     [---
                // right:  ]-----
                Ordering::Greater => Ordering::Greater,
            },
            // left:   ]----- OR ]----- OR   ]---
            // right:    ]--- OR ]----- OR ]-----
            (Bound::Excluded(left), Bound::Excluded(right)) => left.cmp(right),
        }
    }
}

impl Default for LowerBound {
    fn default() -> Self {
        Self(Bound::Unbounded)
    }
}

impl LowerBound {
    /// Initialize a [`LowerBound`] with the given bound.
    ///
    /// These bounds use release-only semantics when comparing versions.
    pub fn new(bound: Bound<Version>) -> Self {
        Self(match bound {
            Bound::Included(version) => Bound::Included(version.only_release()),
            Bound::Excluded(version) => Bound::Excluded(version.only_release()),
            Bound::Unbounded => Bound::Unbounded,
        })
    }
}

impl Deref for LowerBound {
    type Target = Bound<Version>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<LowerBound> for Bound<Version> {
    fn from(bound: LowerBound) -> Self {
        bound.0
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct UpperBound(Bound<Version>);

impl UpperBound {
    /// Return the [`UpperBound`] truncated to the major and minor version.
    fn major_minor(&self) -> Self {
        match &self.0 {
            // Ex) `<=3.10.1` -> `<=3.10`
            Bound::Included(version) => Self(Bound::Included(Version::new(
                version.release().iter().take(2),
            ))),
            // Ex) `<3.10.1` -> `<=3.10` (but `<3.10.0` is `<3.10`)
            Bound::Excluded(version) => {
                if version.release().get(2).is_some_and(|patch| *patch > 0) {
                    Self(Bound::Included(Version::new(
                        version.release().iter().take(2),
                    )))
                } else {
                    Self(Bound::Excluded(Version::new(
                        version.release().iter().take(2),
                    )))
                }
            }
            Bound::Unbounded => Self(Bound::Unbounded),
        }
    }
}

impl PartialOrd for UpperBound {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// See: <https://github.com/pubgrub-rs/pubgrub/blob/4b4b44481c5f93f3233221dc736dd23e67e00992/src/range.rs#L324>
impl Ord for UpperBound {
    fn cmp(&self, other: &Self) -> Ordering {
        let left = self.0.as_ref();
        let right = other.0.as_ref();

        match (left, right) {
            // left:   -----∞
            // right:  -----∞
            (Bound::Unbounded, Bound::Unbounded) => Ordering::Equal,
            // left:   ---]
            // right:  -----∞
            (Bound::Included(_left), Bound::Unbounded) => Ordering::Less,
            // left:   ---[
            // right:  -----∞
            (Bound::Excluded(_left), Bound::Unbounded) => Ordering::Less,
            // left:  -----∞
            // right: ---]
            (Bound::Unbounded, Bound::Included(_right)) => Ordering::Greater,
            // left:   -----] OR -----] OR ---]
            // right:    ---] OR -----] OR -----]
            (Bound::Included(left), Bound::Included(right)) => left.cmp(right),
            (Bound::Excluded(left), Bound::Included(right)) => match left.cmp(right) {
                // left:   ---[
                // right:  -----]
                Ordering::Less => Ordering::Less,
                // left:   -----[
                // right:  -----]
                Ordering::Equal => Ordering::Less,
                // left:   -----[
                // right:  ---]
                Ordering::Greater => Ordering::Greater,
            },
            (Bound::Unbounded, Bound::Excluded(_right)) => Ordering::Greater,
            (Bound::Included(left), Bound::Excluded(right)) => match left.cmp(right) {
                // left:   ---]
                // right:  -----[
                Ordering::Less => Ordering::Less,
                // left:   -----]
                // right:  -----[
                Ordering::Equal => Ordering::Greater,
                // left:   -----]
                // right:  ---[
                Ordering::Greater => Ordering::Greater,
            },
            // left:   -----[ OR -----[ OR ---[
            // right:  ---[   OR -----[ OR -----[
            (Bound::Excluded(left), Bound::Excluded(right)) => left.cmp(right),
        }
    }
}

impl Default for UpperBound {
    fn default() -> Self {
        Self(Bound::Unbounded)
    }
}

impl UpperBound {
    /// Initialize a [`UpperBound`] with the given bound.
    ///
    /// These bounds use release-only semantics when comparing versions.
    pub fn new(bound: Bound<Version>) -> Self {
        Self(match bound {
            Bound::Included(version) => Bound::Included(version.only_release()),
            Bound::Excluded(version) => Bound::Excluded(version.only_release()),
            Bound::Unbounded => Bound::Unbounded,
        })
    }
}

impl Deref for UpperBound {
    type Target = Bound<Version>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<UpperBound> for Bound<Version> {
    fn from(bound: UpperBound) -> Self {
        bound.0
    }
}

#[cfg(test)]
mod tests {
    use std::cmp::Ordering;
    use std::collections::Bound;
    use std::str::FromStr;

    use uv_distribution_filename::WheelFilename;
    use uv_pep440::{Version, VersionSpecifiers};

    use crate::requires_python::{LowerBound, UpperBound};
    use crate::RequiresPython;

    #[test]
    fn requires_python_included() {
        let version_specifiers = VersionSpecifiers::from_str("==3.10.*").unwrap();
        let requires_python = RequiresPython::from_specifiers(&version_specifiers);
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
        let requires_python = RequiresPython::from_specifiers(&version_specifiers);
        let wheel_names = &["dearpygui-1.11.1-cp312-cp312-win_amd64.whl"];
        for wheel_name in wheel_names {
            assert!(
                requires_python.matches_wheel_tag(&WheelFilename::from_str(wheel_name).unwrap()),
                "{wheel_name}"
            );
        }

        let version_specifiers = VersionSpecifiers::from_str("==3.12.6").unwrap();
        let requires_python = RequiresPython::from_specifiers(&version_specifiers);
        let wheel_names = &["lxml-5.3.0-cp312-cp312-musllinux_1_2_aarch64.whl"];
        for wheel_name in wheel_names {
            assert!(
                requires_python.matches_wheel_tag(&WheelFilename::from_str(wheel_name).unwrap()),
                "{wheel_name}"
            );
        }

        let version_specifiers = VersionSpecifiers::from_str("==3.12").unwrap();
        let requires_python = RequiresPython::from_specifiers(&version_specifiers);
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
        let requires_python = RequiresPython::from_specifiers(&version_specifiers);
        let wheel_names = &[
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
            assert!(
                !requires_python.matches_wheel_tag(&WheelFilename::from_str(wheel_name).unwrap()),
                "{wheel_name}"
            );
        }

        let version_specifiers = VersionSpecifiers::from_str(">=3.12.3").unwrap();
        let requires_python = RequiresPython::from_specifiers(&version_specifiers);
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
            let requires_python = RequiresPython::from_specifiers(&version_specifiers);
            assert_eq!(requires_python.is_exact_without_patch(), expected);
        }
    }
}
