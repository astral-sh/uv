use std::borrow::Borrow;
use std::str::FromStr;

use itertools::Itertools;
use rustc_hash::FxHashMap;

use uv_normalize::{ExtraName, GroupName, PackageName};
use uv_pep508::{
    ExtraOperator, MarkerEnvironment, MarkerEnvironmentBuilder, MarkerExpression, MarkerOperator,
    MarkerTree,
};
use uv_pypi_types::{ConflictItem, ConflictPackage, Conflicts};

use crate::ResolveError;

/// A representation of a marker for use in universal resolution.
///
/// (This degrades gracefully to a standard PEP 508 marker in the case of
/// non-universal resolution.)
///
/// This universal marker is meant to combine both a PEP 508 marker and a
/// marker for conflicting extras/groups. The latter specifically expresses
/// whether a particular edge in a dependency graph should be followed
/// depending on the activated extras and groups.
///
/// A universal marker evaluates to true only when *both* its PEP 508 marker
/// and its conflict marker evaluate to true.
#[derive(Default, Copy, Clone, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub struct UniversalMarker {
    /// The full combined PEP 508 and "conflict" marker.
    ///
    /// In the original design, the PEP 508 marker was kept separate
    /// from the conflict marker, since the conflict marker is not really
    /// specified by PEP 508. However, this approach turned out to be
    /// bunk because the conflict marker vary depending on which part of
    /// the PEP 508 marker is true. For example, you might have a different
    /// conflict marker for one platform versus the other. The only way to
    /// resolve this is to combine them both into one marker.
    ///
    /// The downside of this is that since conflict markers aren't part of
    /// PEP 508, combining them is pretty weird. We could combine them into
    /// a new type of marker that isn't PEP 508. But it's not clear what the
    /// best design for that is, and at the time of writing, it would have
    /// been a lot of additional work. (Our PEP 508 marker implementation is
    /// rather sophisticated given its boolean simplification capabilities.
    /// So leveraging all that work is a huge shortcut.) So to accomplish
    /// this, we technically preserve PEP 508 compatibility but abuse the
    /// `extra` attribute to encode conflicts.
    ///
    /// So for example, if a particular dependency should only be activated
    /// on `Darwin` and when the extra `x1` for package `foo` is enabled,
    /// then its "universal" marker looks like this:
    ///
    /// ```text
    /// sys_platform == 'Darwin' and extra == 'extra-3-foo-x1'
    /// ```
    ///
    /// Then, when `uv sync --extra x1` is called, we encode that was
    /// `extra-3-foo-x1` and pass it as-needed when evaluating this marker.
    ///
    /// Why `extra-3-foo-x1`?
    ///
    /// * The `extra` prefix is there to distinguish it from `group`.
    /// * The `3` is there to indicate the length of the package name,
    ///   in bytes. This isn't strictly necessary for encoding, but
    ///   is required if we were ever to need to decode a package and
    ///   extra/group name from a conflict marker.
    /// * The `foo` package name ensures we namespace the extra/group name,
    ///   since multiple packages can have the same extra/group name.
    ///
    /// We only use alphanumeric characters and hyphens in order to limit
    /// ourselves to valid extra names. (If we could use other characters then
    /// that would avoid the need to encode the length of the package name.)
    ///
    /// So while the above marker is still technically valid from a PEP 508
    /// stand-point, evaluating it requires uv's custom encoding of extras (and
    /// groups).
    marker: MarkerTree,
    /// The strictly PEP 508 version of `marker`. Basically, `marker`, but
    /// without any extras in it. This could be computed on demand (and
    /// that's what we used to do), but we do it enough that it was causing a
    /// regression in some cases.
    pep508: MarkerTree,
}

impl UniversalMarker {
    /// A constant universal marker that always evaluates to `true`.
    pub(crate) const TRUE: Self = Self {
        marker: MarkerTree::TRUE,
        pep508: MarkerTree::TRUE,
    };

    /// A constant universal marker that always evaluates to `false`.
    pub(crate) const FALSE: Self = Self {
        marker: MarkerTree::FALSE,
        pep508: MarkerTree::FALSE,
    };

    /// Creates a new universal marker from its constituent pieces.
    pub(crate) fn new(mut pep508_marker: MarkerTree, conflict_marker: ConflictMarker) -> Self {
        pep508_marker.and(conflict_marker.marker);
        Self::from_combined(pep508_marker)
    }

    /// Creates a new universal marker from a marker that has already been
    /// combined from a PEP 508 and conflict marker.
    pub(crate) fn from_combined(marker: MarkerTree) -> Self {
        Self {
            marker,
            pep508: marker.without_extras(),
        }
    }

    /// Combine this universal marker with the one given in a way that unions
    /// them. That is, the updated marker will evaluate to `true` if `self` or
    /// `other` evaluate to `true`.
    pub(crate) fn or(&mut self, other: Self) {
        self.marker.or(other.marker);
        self.pep508.or(other.pep508);
    }

    /// Combine this universal marker with the one given in a way that
    /// intersects them. That is, the updated marker will evaluate to `true` if
    /// `self` and `other` evaluate to `true`.
    pub(crate) fn and(&mut self, other: Self) {
        self.marker.and(other.marker);
        self.pep508.and(other.pep508);
    }

    /// Imbibes the world knowledge expressed by `conflicts` into this marker.
    ///
    /// This will effectively simplify the conflict marker in this universal
    /// marker. In particular, it enables simplifying based on the fact that no
    /// two items from the same set in the given conflicts can be active at a
    /// given time.
    pub(crate) fn imbibe(&mut self, conflicts: ConflictMarker) {
        let self_marker = self.marker;
        self.marker = conflicts.marker;
        self.marker.implies(self_marker);
        self.pep508 = self.marker.without_extras();
    }

    /// Assumes that a given extra/group for the given package is activated.
    ///
    /// This may simplify the conflicting marker component of this universal
    /// marker.
    pub(crate) fn assume_conflict_item(&mut self, item: &ConflictItem) {
        match *item.conflict() {
            ConflictPackage::Extra(ref extra) => self.assume_extra(item.package(), extra),
            ConflictPackage::Group(ref group) => self.assume_group(item.package(), group),
        }
        self.pep508 = self.marker.without_extras();
    }

    /// Assumes that a given extra/group for the given package is not
    /// activated.
    ///
    /// This may simplify the conflicting marker component of this universal
    /// marker.
    pub(crate) fn assume_not_conflict_item(&mut self, item: &ConflictItem) {
        match *item.conflict() {
            ConflictPackage::Extra(ref extra) => self.assume_not_extra(item.package(), extra),
            ConflictPackage::Group(ref group) => self.assume_not_group(item.package(), group),
        }
        self.pep508 = self.marker.without_extras();
    }

    /// Assumes that a given extra for the given package is activated.
    ///
    /// This may simplify the conflicting marker component of this universal
    /// marker.
    pub(crate) fn assume_extra(&mut self, package: &PackageName, extra: &ExtraName) {
        let extra = encode_package_extra(package, extra);
        self.marker = self
            .marker
            .simplify_extras_with(|candidate| *candidate == extra);
        self.pep508 = self.marker.without_extras();
    }

    /// Assumes that a given extra for the given package is not activated.
    ///
    /// This may simplify the conflicting marker component of this universal
    /// marker.
    pub(crate) fn assume_not_extra(&mut self, package: &PackageName, extra: &ExtraName) {
        let extra = encode_package_extra(package, extra);
        self.marker = self
            .marker
            .simplify_not_extras_with(|candidate| *candidate == extra);
        self.pep508 = self.marker.without_extras();
    }

    /// Assumes that a given group for the given package is activated.
    ///
    /// This may simplify the conflicting marker component of this universal
    /// marker.
    pub(crate) fn assume_group(&mut self, package: &PackageName, group: &GroupName) {
        let extra = encode_package_group(package, group);
        self.marker = self
            .marker
            .simplify_extras_with(|candidate| *candidate == extra);
        self.pep508 = self.marker.without_extras();
    }

    /// Assumes that a given group for the given package is not activated.
    ///
    /// This may simplify the conflicting marker component of this universal
    /// marker.
    pub(crate) fn assume_not_group(&mut self, package: &PackageName, group: &GroupName) {
        let extra = encode_package_group(package, group);
        self.marker = self
            .marker
            .simplify_not_extras_with(|candidate| *candidate == extra);
        self.pep508 = self.marker.without_extras();
    }

    /// Returns true if this universal marker will always evaluate to `true`.
    pub(crate) fn is_true(self) -> bool {
        self.marker.is_true()
    }

    /// Returns true if this universal marker will always evaluate to `false`.
    pub(crate) fn is_false(self) -> bool {
        self.marker.is_false()
    }

    /// Returns true if this universal marker is disjoint with the one given.
    ///
    /// Two universal markers are disjoint when it is impossible for them both
    /// to evaluate to `true` simultaneously.
    pub(crate) fn is_disjoint(self, other: Self) -> bool {
        self.marker.is_disjoint(other.marker)
    }

    /// Returns true if this universal marker is satisfied by the given marker
    /// environment.
    ///
    /// This should only be used when evaluating a marker that is known not to
    /// have any extras. For example, the PEP 508 markers on a fork.
    pub(crate) fn evaluate_no_extras(self, env: &MarkerEnvironment) -> bool {
        self.marker.evaluate(env, &[])
    }

    /// Returns true if this universal marker is satisfied by the given marker
    /// environment and list of activated extras and groups.
    ///
    /// The activated extras and groups should be the complete set activated
    /// for a particular context. And each extra and group must be scoped to
    /// the particular package that it's enabled for.
    pub(crate) fn evaluate<P, E, G>(
        self,
        env: &MarkerEnvironment,
        extras: impl Iterator<Item = (P, E)>,
        groups: impl Iterator<Item = (P, G)>,
    ) -> bool
    where
        P: Borrow<PackageName>,
        E: Borrow<ExtraName>,
        G: Borrow<GroupName>,
    {
        let extras =
            extras.map(|(package, extra)| encode_package_extra(package.borrow(), extra.borrow()));
        let groups =
            groups.map(|(package, group)| encode_package_group(package.borrow(), group.borrow()));
        self.marker
            .evaluate(env, &extras.chain(groups).collect::<Vec<ExtraName>>())
    }

    /// Returns the internal marker that combines both the PEP 508
    /// and conflict marker.
    pub fn combined(self) -> MarkerTree {
        self.marker
    }

    /// Returns the PEP 508 marker for this universal marker.
    ///
    /// One should be cautious using this. Generally speaking, it should only
    /// be used when one knows universal resolution isn't in effect. When
    /// universal resolution is enabled (i.e., there may be multiple forks
    /// producing different versions of the same package), then one should
    /// always use a universal marker since it accounts for all possible ways
    /// for a package to be installed.
    pub fn pep508(self) -> MarkerTree {
        self.pep508
    }

    /// Returns the non-PEP 508 marker expression that represents conflicting
    /// extras/groups.
    ///
    /// Like with `UniversalMarker::pep508`, one should be cautious when using
    /// this. It is generally always wrong to consider conflicts in isolation
    /// from PEP 508 markers. But this can be useful for detecting failure
    /// cases. For example, the code for emitting a `ResolverOutput` (even a
    /// universal one) in a `requirements.txt` format checks for the existence
    /// of non-trivial conflict markers and fails if any are found. (Because
    /// conflict markers cannot be represented in the `requirements.txt`
    /// format.)
    pub(crate) fn conflict(self) -> ConflictMarker {
        ConflictMarker {
            marker: self.marker.only_extras(),
        }
    }
}

impl std::fmt::Debug for UniversalMarker {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.marker, f)
    }
}

/// A marker that is only for representing conflicting extras/groups.
///
/// This encapsulates the encoding of extras and groups into PEP 508
/// markers.
#[derive(Default, Clone, Copy, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub struct ConflictMarker {
    marker: MarkerTree,
}

impl ConflictMarker {
    /// A constant conflict marker that always evaluates to `true`.
    pub const TRUE: Self = Self {
        marker: MarkerTree::TRUE,
    };

    /// A constant conflict marker that always evaluates to `false`.
    pub const FALSE: Self = Self {
        marker: MarkerTree::FALSE,
    };

    /// Creates a new conflict marker from the declared conflicts provided.
    pub fn from_conflicts(conflicts: &Conflicts) -> Self {
        if conflicts.is_empty() {
            return Self::TRUE;
        }
        let mut marker = Self::TRUE;
        for set in conflicts.iter() {
            for (item1, item2) in set.iter().tuple_combinations() {
                let pair = Self::from_conflict_item(item1)
                    .negate()
                    .or(Self::from_conflict_item(item2).negate());
                marker = marker.and(pair);
            }
        }
        marker
    }

    /// Create a conflict marker that is true only when the given extra or
    /// group (for a specific package) is activated.
    pub fn from_conflict_item(item: &ConflictItem) -> Self {
        match *item.conflict() {
            ConflictPackage::Extra(ref extra) => Self::extra(item.package(), extra),
            ConflictPackage::Group(ref group) => Self::group(item.package(), group),
        }
    }

    /// Create a conflict marker that is true only when the given extra for the
    /// given package is activated.
    pub fn extra(package: &PackageName, extra: &ExtraName) -> Self {
        let operator = uv_pep508::ExtraOperator::Equal;
        let name = uv_pep508::MarkerValueExtra::Extra(encode_package_extra(package, extra));
        let expr = uv_pep508::MarkerExpression::Extra { operator, name };
        let marker = MarkerTree::expression(expr);
        Self { marker }
    }

    /// Create a conflict marker that is true only when the given group for the
    /// given package is activated.
    pub fn group(package: &PackageName, group: &GroupName) -> Self {
        let operator = uv_pep508::ExtraOperator::Equal;
        let name = uv_pep508::MarkerValueExtra::Extra(encode_package_group(package, group));
        let expr = uv_pep508::MarkerExpression::Extra { operator, name };
        let marker = MarkerTree::expression(expr);
        Self { marker }
    }

    /// Returns a new conflict marker that is the negation of this one.
    #[must_use]
    pub fn negate(self) -> Self {
        Self {
            marker: self.marker.negate(),
        }
    }

    /// Returns a new conflict marker corresponding to the union of `self` and
    /// `other`.
    #[must_use]
    pub fn or(self, other: Self) -> Self {
        let mut marker = self.marker;
        marker.or(other.marker);
        Self { marker }
    }

    /// Returns a new conflict marker corresponding to the intersection of
    /// `self` and `other`.
    #[must_use]
    pub fn and(self, other: Self) -> Self {
        let mut marker = self.marker;
        marker.and(other.marker);
        Self { marker }
    }

    /// Returns a new conflict marker corresponding to the logical implication
    /// of `self` and the given consequent.
    ///
    /// If the conflict marker returned is always `true`, then it can be said
    /// that `self` implies `consequent`.
    #[must_use]
    pub fn implies(self, other: Self) -> Self {
        let mut marker = self.marker;
        marker.implies(other.marker);
        Self { marker }
    }

    /// Returns true if this conflict marker will always evaluate to `true`.
    pub fn is_true(self) -> bool {
        self.marker.is_true()
    }

    /// Returns true if this conflict marker will always evaluate to `false`.
    pub fn is_false(self) -> bool {
        self.marker.is_false()
    }

    /// Returns true if this conflict marker is satisfied by the given
    /// list of activated extras and groups.
    pub(crate) fn evaluate<P, E, G>(self, extras: &[(P, E)], groups: &[(P, G)]) -> bool
    where
        P: Borrow<PackageName>,
        E: Borrow<ExtraName>,
        G: Borrow<GroupName>,
    {
        static DUMMY: std::sync::LazyLock<MarkerEnvironment> = std::sync::LazyLock::new(|| {
            MarkerEnvironment::try_from(MarkerEnvironmentBuilder {
                implementation_name: "",
                implementation_version: "3.7",
                os_name: "linux",
                platform_machine: "",
                platform_python_implementation: "",
                platform_release: "",
                platform_system: "",
                platform_version: "",
                python_full_version: "3.7",
                python_version: "3.7",
                sys_platform: "linux",
            })
            .unwrap()
        });
        let extras = extras
            .iter()
            .map(|(package, extra)| encode_package_extra(package.borrow(), extra.borrow()));
        let groups = groups
            .iter()
            .map(|(package, group)| encode_package_group(package.borrow(), group.borrow()));
        self.marker
            .evaluate(&DUMMY, &extras.chain(groups).collect::<Vec<ExtraName>>())
    }

    /// Returns inclusion and exclusion (respectively) conflict items parsed
    /// from this conflict marker.
    ///
    /// This returns an error if any `extra` could not be parsed as a valid
    /// encoded conflict extra.
    pub(crate) fn filter_rules(
        self,
    ) -> Result<(Vec<ConflictItem>, Vec<ConflictItem>), ResolveError> {
        let (mut raw_include, mut raw_exclude) = (vec![], vec![]);
        self.marker.visit_extras(|op, extra| {
            match op {
                MarkerOperator::Equal => raw_include.push(extra.to_owned()),
                MarkerOperator::NotEqual => raw_exclude.push(extra.to_owned()),
                // OK by the contract of `MarkerTree::visit_extras`.
                _ => unreachable!(),
            }
        });
        let include = raw_include
            .into_iter()
            .map(|extra| ParsedRawExtra::parse(&extra).and_then(|parsed| parsed.to_conflict_item()))
            .collect::<Result<Vec<_>, _>>()?;
        let exclude = raw_exclude
            .into_iter()
            .map(|extra| ParsedRawExtra::parse(&extra).and_then(|parsed| parsed.to_conflict_item()))
            .collect::<Result<Vec<_>, _>>()?;
        Ok((include, exclude))
    }
}

impl std::fmt::Debug for ConflictMarker {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        // This is a little more succinct than the default.
        write!(f, "ConflictMarker({:?})", self.marker)
    }
}

/// Encodes the given package name and its corresponding extra into a valid
/// `extra` value in a PEP 508 marker.
fn encode_package_extra(package: &PackageName, extra: &ExtraName) -> ExtraName {
    // This is OK because `PackageName` and `ExtraName` have the same
    // validation rules, and we combine them in a way that always results in a
    // valid name.
    //
    // Note also that we encode the length of the package name (in bytes) into
    // the encoded extra name as well. This ensures we can parse out both the
    // package and extra name if necessary. If we didn't do this, then some
    // cases could be ambiguous since our field delimiter (`-`) is also a valid
    // character in `package` or `extra` values. But if we know the length of
    // the package name, we can always parse each field unambiguously.
    let package_len = package.as_str().len();
    ExtraName::from_owned(format!("extra-{package_len}-{package}-{extra}")).unwrap()
}

/// Encodes the given package name and its corresponding group into a valid
/// `extra` value in a PEP 508 marker.
fn encode_package_group(package: &PackageName, group: &GroupName) -> ExtraName {
    // See `encode_package_extra`, the same considerations apply here.
    let package_len = package.as_str().len();
    ExtraName::from_owned(format!("group-{package_len}-{package}-{group}")).unwrap()
}

#[derive(Debug)]
enum ParsedRawExtra<'a> {
    Extra { package: &'a str, extra: &'a str },
    Group { package: &'a str, group: &'a str },
}

impl<'a> ParsedRawExtra<'a> {
    fn parse(raw_extra: &'a ExtraName) -> Result<Self, ResolveError> {
        fn mkerr(raw_extra: &ExtraName, reason: impl Into<String>) -> ResolveError {
            let raw_extra = raw_extra.to_owned();
            let reason = reason.into();
            ResolveError::InvalidExtraInConflictMarker { reason, raw_extra }
        }

        let raw = raw_extra.as_str();
        let Some((kind, tail)) = raw.split_once('-') else {
            return Err(mkerr(
                raw_extra,
                "expected to find leading `extra-` or `group-`",
            ));
        };
        let Some((len, tail)) = tail.split_once('-') else {
            return Err(mkerr(
                raw_extra,
                "expected to find `{number}-` after leading `extra-` or `group-`",
            ));
        };
        let len = len.parse::<usize>().map_err(|_| {
            mkerr(
                raw_extra,
                format!("found package length number `{len}`, but could not parse into integer"),
            )
        })?;
        let Some((package, tail)) = tail.split_at_checked(len) else {
            return Err(mkerr(
                raw_extra,
                format!(
                    "expected at least {len} bytes for package name, but found {found}",
                    found = tail.len()
                ),
            ));
        };
        if !tail.starts_with('-') {
            return Err(mkerr(
                raw_extra,
                format!("expected `-` after package name `{package}`"),
            ));
        }
        let tail = &tail[1..];
        match kind {
            "extra" => Ok(ParsedRawExtra::Extra {
                package,
                extra: tail,
            }),
            "group" => Ok(ParsedRawExtra::Group {
                package,
                group: tail,
            }),
            _ => Err(mkerr(
                raw_extra,
                format!("unrecognized kind `{kind}` (must be `extra` or `group`)"),
            )),
        }
    }

    fn to_conflict_item(&self) -> Result<ConflictItem, ResolveError> {
        let package = PackageName::from_str(self.package()).map_err(|name_error| {
            ResolveError::InvalidValueInConflictMarker {
                kind: "package",
                name_error,
            }
        })?;
        match self {
            Self::Extra { extra, .. } => {
                let extra = ExtraName::from_str(extra).map_err(|name_error| {
                    ResolveError::InvalidValueInConflictMarker {
                        kind: "extra",
                        name_error,
                    }
                })?;
                Ok(ConflictItem::from((package, extra)))
            }
            Self::Group { group, .. } => {
                let group = GroupName::from_str(group).map_err(|name_error| {
                    ResolveError::InvalidValueInConflictMarker {
                        kind: "group",
                        name_error,
                    }
                })?;
                Ok(ConflictItem::from((package, group)))
            }
        }
    }

    fn package(&self) -> &'a str {
        match self {
            Self::Extra { package, .. } => package,
            Self::Group { package, .. } => package,
        }
    }
}

/// Resolve the conflict markers in a [`MarkerTree`] based on the conditions under which each
/// conflict item is known to be true.
///
/// For example, if the `cpu` extra is known to be enabled when `sys_platform == 'darwin'`, then
/// given the combined marker `python_version >= '3.8' and extra == 'extra-7-project-cpu'`, this
/// method would return `python_version >= '3.8' and sys_platform == 'darwin'`.
///
/// If a conflict item isn't present in the map of known conflicts, it's assumed to be false in all
/// environments.
pub(crate) fn resolve_conflicts(
    marker: MarkerTree,
    known_conflicts: &FxHashMap<ConflictItem, MarkerTree>,
) -> MarkerTree {
    if marker.is_true() || marker.is_false() {
        return marker;
    }

    let mut transformed = MarkerTree::FALSE;

    // Convert the marker to DNF, then re-build it.
    for dnf in marker.to_dnf() {
        let mut or = MarkerTree::TRUE;

        for marker in dnf {
            let MarkerExpression::Extra {
                ref operator,
                ref name,
            } = marker
            else {
                or.and(MarkerTree::expression(marker));
                continue;
            };

            let Some(name) = name.as_extra() else {
                or.and(MarkerTree::expression(marker));
                continue;
            };

            // Given an extra marker (like `extra == 'extra-7-project-cpu'`), search for the
            // corresponding conflict; once found, inline the marker of conditions under which the
            // conflict is known to be true.
            let mut found = false;
            for (conflict_item, conflict_marker) in known_conflicts {
                // Search for the conflict item as an extra.
                if let Some(extra) = conflict_item.extra() {
                    let package = conflict_item.package();
                    let encoded = encode_package_extra(package, extra);
                    if encoded == *name {
                        match operator {
                            ExtraOperator::Equal => {
                                or.and(*conflict_marker);
                                found = true;
                                break;
                            }
                            ExtraOperator::NotEqual => {
                                or.and(conflict_marker.negate());
                                found = true;
                                break;
                            }
                        }
                    }
                }

                // Search for the conflict item as a group.
                if let Some(group) = conflict_item.group() {
                    let package = conflict_item.package();
                    let encoded = encode_package_group(package, group);
                    if encoded == *name {
                        match operator {
                            ExtraOperator::Equal => {
                                or.and(*conflict_marker);
                                found = true;
                                break;
                            }
                            ExtraOperator::NotEqual => {
                                or.and(conflict_marker.negate());
                                found = true;
                                break;
                            }
                        }
                    }
                }
            }

            // If we didn't find the marker in the list of known conflicts, assume it's always
            // false.
            if !found {
                match operator {
                    ExtraOperator::Equal => {
                        or.and(MarkerTree::FALSE);
                    }
                    ExtraOperator::NotEqual => {
                        or.and(MarkerTree::TRUE);
                    }
                }
            }
        }

        transformed.or(or);
    }

    transformed
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    use uv_pypi_types::ConflictSet;

    /// Creates a collection of declared conflicts from the sets
    /// provided.
    fn create_conflicts(it: impl IntoIterator<Item = ConflictSet>) -> Conflicts {
        let mut conflicts = Conflicts::empty();
        for set in it {
            conflicts.push(set);
        }
        conflicts
    }

    /// Creates a single set of conflicting items.
    ///
    /// For convenience, this always creates conflicting items with a package
    /// name of `foo` and with the given string as the extra name.
    fn create_set<'a>(it: impl IntoIterator<Item = &'a str>) -> ConflictSet {
        let items = it
            .into_iter()
            .map(|extra| (create_package("pkg"), create_extra(extra)))
            .map(ConflictItem::from)
            .collect::<Vec<ConflictItem>>();
        ConflictSet::try_from(items).unwrap()
    }

    /// Shortcut for creating a package name.
    fn create_package(name: &str) -> PackageName {
        PackageName::from_str(name).unwrap()
    }

    /// Shortcut for creating an extra name.
    fn create_extra(name: &str) -> ExtraName {
        ExtraName::from_str(name).unwrap()
    }

    /// Shortcut for creating a conflict marker from an extra name.
    fn create_extra_marker(name: &str) -> ConflictMarker {
        ConflictMarker::extra(&create_package("pkg"), &create_extra(name))
    }

    /// Shortcut for creating a conflict item from an extra name.
    fn create_extra_item(name: &str) -> ConflictItem {
        ConflictItem::from((create_package("pkg"), create_extra(name)))
    }

    /// Shortcut for creating a conflict map.
    fn create_known_conflicts<'a>(
        it: impl IntoIterator<Item = (&'a str, &'a str)>,
    ) -> FxHashMap<ConflictItem, MarkerTree> {
        it.into_iter()
            .map(|(extra, marker)| {
                (
                    create_extra_item(extra),
                    MarkerTree::from_str(marker).unwrap(),
                )
            })
            .collect()
    }

    /// Returns a string representation of the given conflict marker.
    ///
    /// This is just the underlying marker. And if it's `true`, then a
    /// non-conforming `true` string is returned. (Which is fine since
    /// this is just for tests.)
    fn tostr(cm: ConflictMarker) -> String {
        cm.marker
            .try_to_string()
            .unwrap_or_else(|| "true".to_string())
    }

    /// This tests the conversion from declared conflicts into a conflict
    /// marker. This is used to describe "world knowledge" about which
    /// extras/groups are and aren't allowed to be activated together.
    #[test]
    fn conflicts_as_marker() {
        let conflicts = create_conflicts([create_set(["foo", "bar"])]);
        let cm = ConflictMarker::from_conflicts(&conflicts);
        assert_eq!(
            tostr(cm),
            "extra != 'extra-3-pkg-foo' or extra != 'extra-3-pkg-bar'"
        );

        let conflicts = create_conflicts([create_set(["foo", "bar", "baz"])]);
        let cm = ConflictMarker::from_conflicts(&conflicts);
        assert_eq!(
            tostr(cm),
            "(extra != 'extra-3-pkg-baz' and extra != 'extra-3-pkg-foo') \
             or (extra != 'extra-3-pkg-bar' and extra != 'extra-3-pkg-foo') \
             or (extra != 'extra-3-pkg-bar' and extra != 'extra-3-pkg-baz')",
        );

        let conflicts = create_conflicts([create_set(["foo", "bar"]), create_set(["fox", "ant"])]);
        let cm = ConflictMarker::from_conflicts(&conflicts);
        assert_eq!(
            tostr(cm),
            "(extra != 'extra-3-pkg-bar' and extra != 'extra-3-pkg-fox') or \
             (extra != 'extra-3-pkg-ant' and extra != 'extra-3-pkg-foo') or \
             (extra != 'extra-3-pkg-ant' and extra != 'extra-3-pkg-bar') or \
             (extra == 'extra-3-pkg-bar' and extra != 'extra-3-pkg-foo' and extra != 'extra-3-pkg-fox')",
        );
        // I believe because markers are put into DNF, the marker we get here
        // is a lot bigger than what we might expect. Namely, this is how it's
        // constructed:
        //
        //     (extra != 'extra-3-pkg-foo' or extra != 'extra-3-pkg-bar')
        //     and (extra != 'extra-3-pkg-fox' or extra != 'extra-3-pkg-ant')
        //
        // In other words, you can't have both `foo` and `bar` active, and you
        // can't have both `fox` and `ant` active. But any other combination
        // is valid. So let's step through all of them to make sure the marker
        // below gives the expected result. (I did this because it's not at all
        // obvious to me that the above two markers are equivalent.)
        let disallowed = [
            vec!["foo", "bar"],
            vec!["fox", "ant"],
            vec!["foo", "fox", "bar"],
            vec!["foo", "ant", "bar"],
            vec!["ant", "foo", "fox"],
            vec!["ant", "bar", "fox"],
            vec!["foo", "bar", "fox", "ant"],
        ];
        for extra_names in disallowed {
            let extras = extra_names
                .iter()
                .copied()
                .map(|name| (create_package("pkg"), create_extra(name)))
                .collect::<Vec<(PackageName, ExtraName)>>();
            let groups = Vec::<(PackageName, GroupName)>::new();
            assert!(
                !cm.evaluate(&extras, &groups),
                "expected `{extra_names:?}` to evaluate to `false` in `{cm:?}`"
            );
        }
        let allowed = [
            vec![],
            vec!["foo"],
            vec!["bar"],
            vec!["fox"],
            vec!["ant"],
            vec!["foo", "fox"],
            vec!["foo", "ant"],
            vec!["bar", "fox"],
            vec!["bar", "ant"],
        ];
        for extra_names in allowed {
            let extras = extra_names
                .iter()
                .copied()
                .map(|name| (create_package("pkg"), create_extra(name)))
                .collect::<Vec<(PackageName, ExtraName)>>();
            let groups = Vec::<(PackageName, GroupName)>::new();
            assert!(
                cm.evaluate(&extras, &groups),
                "expected `{extra_names:?}` to evaluate to `true` in `{cm:?}`"
            );
        }
    }

    /// This tests conflict marker simplification after "imbibing" world
    /// knowledge about which extras/groups cannot be activated together.
    #[test]
    fn imbibe() {
        let conflicts = create_conflicts([create_set(["foo", "bar"])]);
        let conflicts_marker = ConflictMarker::from_conflicts(&conflicts);
        let foo = create_extra_marker("foo");
        let bar = create_extra_marker("bar");

        // In this case, we simulate a dependency whose conflict marker
        // is just repeating the fact that conflicting extras cannot
        // both be activated. So this one simplifies to `true`.
        let mut dep_conflict_marker =
            UniversalMarker::new(MarkerTree::TRUE, foo.negate().or(bar.negate()));
        assert_eq!(
            format!("{dep_conflict_marker:?}"),
            "extra != 'extra-3-pkg-foo' or extra != 'extra-3-pkg-bar'"
        );
        dep_conflict_marker.imbibe(conflicts_marker);
        assert_eq!(format!("{dep_conflict_marker:?}"), "true");
    }

    #[test]
    fn resolve() {
        let known_conflicts = create_known_conflicts([("foo", "sys_platform == 'darwin'")]);
        let cm = MarkerTree::from_str("(python_version >= '3.10' and extra == 'extra-3-pkg-foo') or (python_version < '3.10' and extra != 'extra-3-pkg-foo')").unwrap();
        let cm = resolve_conflicts(cm, &known_conflicts);
        assert_eq!(
            cm.try_to_string().as_deref(),
            Some(
                "(python_full_version < '3.10' and sys_platform != 'darwin') or (python_full_version >= '3.10' and sys_platform == 'darwin')"
            )
        );

        let cm = MarkerTree::from_str("python_version >= '3.10' and extra == 'extra-3-pkg-foo'")
            .unwrap();
        let cm = resolve_conflicts(cm, &known_conflicts);
        assert_eq!(
            cm.try_to_string().as_deref(),
            Some("python_full_version >= '3.10' and sys_platform == 'darwin'")
        );

        let cm = MarkerTree::from_str("python_version >= '3.10' and extra == 'extra-3-pkg-bar'")
            .unwrap();
        let cm = resolve_conflicts(cm, &known_conflicts);
        assert!(cm.is_false());
    }
}
