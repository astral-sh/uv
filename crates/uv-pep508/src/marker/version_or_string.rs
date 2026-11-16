//! Emit PEP 508 comparisons which cover both parsed versions and opaque strings.
use std::ops::Bound;

use arcstr::ArcStr;
use uv_pep440::{Version, VersionSpecifier, canonicalize_version_ranges};
use version_ranges::Ranges;

use crate::MarkerOperator;

/// A concrete value of a `Version | String` marker. Ranges on the two variants
/// independently encode PEP 440 comparisons and their raw-string fallback.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum VersionOrStringValue {
    /// A parsed environment value. Put versions first so restricting a marker under
    /// an assumption prefers the numeric release when assigning unconstrained cases.
    Version(Version),
    /// The original environment value when it cannot be parsed as a version.
    Opaque(ArcStr),
}

/// Ordered comparisons use the order of parsed versions. In particular, a
/// post-release sorts after its final release. Equality, wildcards, and compatible releases use PEP 440 specifiers.
fn numeric_range(operator: MarkerOperator, literal: &ArcStr) -> Option<Ranges<Version>> {
    let versions = match operator {
        MarkerOperator::GreaterThan
        | MarkerOperator::GreaterEqual
        | MarkerOperator::LessThan
        | MarkerOperator::LessEqual => operator.ordered_range(literal.parse::<Version>().ok()?),
        MarkerOperator::Equal | MarkerOperator::NotEqual | MarkerOperator::TildeEqual => {
            let specifier = VersionSpecifier::from_pattern(
                operator.to_pep440_operator()?,
                literal.parse().ok()?,
            )
            .ok()?;
            Ranges::from(specifier)
        }
        _ => return None,
    };
    Some(canonicalize_version_ranges(&versions).unwrap_or(versions))
}

/// Opaque values sort after all versions and begin at the empty string.
const FIRST_OPAQUE: VersionOrStringValue = VersionOrStringValue::Opaque(arcstr::literal!(""));

pub(super) fn range(
    operator: MarkerOperator,
    literal: &ArcStr,
) -> Option<Ranges<VersionOrStringValue>> {
    let version = |v: Version| VersionOrStringValue::Version(v.without_trailing_zeros());
    let numeric = if let Some(numeric) = numeric_range(operator, literal) {
        numeric
    } else if literal.parse::<Version>().is_ok() {
        // A malformed compatible comparison such as `~=2` falls back to raw equality.
        return None;
    } else if operator == MarkerOperator::NotEqual {
        Ranges::full()
    } else {
        Ranges::empty()
    };
    let versions = numeric
        .into_iter()
        .map(|(low, high)| {
            let high = match high {
                Bound::Unbounded => Bound::Excluded(FIRST_OPAQUE),
                bound => bound.map(version),
            };
            (low.map(version), high)
        })
        .collect::<Ranges<_>>();
    let equal = if literal.parse::<Version>().is_ok() {
        Ranges::empty()
    } else {
        Ranges::singleton(VersionOrStringValue::Opaque(literal.clone()))
    };
    let opaque = match operator {
        MarkerOperator::GreaterThan | MarkerOperator::LessThan => Ranges::empty(),
        MarkerOperator::NotEqual => {
            Ranges::higher_than(FIRST_OPAQUE).intersection(&equal.complement())
        }
        _ => equal,
    };
    Some(versions.union(&opaque))
}

fn numeric_projection(range: &Ranges<VersionOrStringValue>) -> Ranges<Version> {
    let numeric = range.intersection(&Ranges::strictly_lower_than(FIRST_OPAQUE));
    let version = |bound| match bound {
        Bound::Included(VersionOrStringValue::Version(v)) => Bound::Included(v),
        Bound::Excluded(VersionOrStringValue::Version(v)) => Bound::Excluded(v),
        _ => Bound::Unbounded,
    };
    let numeric = numeric
        .into_iter()
        .map(|(a, b)| (version(a), version(b)))
        .collect();
    canonicalize_version_ranges(&numeric).unwrap_or(numeric)
}

type Comparison = (MarkerOperator, ArcStr);
type Dnf = Vec<Vec<Comparison>>;

fn opaque_points(range: &Ranges<VersionOrStringValue>) -> Option<Vec<ArcStr>> {
    range
        .intersection(&Ranges::higher_than(FIRST_OPAQUE))
        .into_iter()
        .map(|(low, high)| match (low, high) {
            (
                Bound::Included(VersionOrStringValue::Opaque(a)),
                Bound::Included(VersionOrStringValue::Opaque(b)),
            ) if a == b => Some(a),
            _ => None,
        })
        .collect()
}

/// Emit exact clauses or leave an algebraic guard for the fallible PEP 508 writer.
pub(super) fn comparisons(range: &Ranges<VersionOrStringValue>) -> Option<Dnf> {
    use MarkerOperator::{Equal, GreaterEqual, NotEqual};

    let versions = numeric_projection(range);
    let included = opaque_points(range);
    let excluded = opaque_points(&range.complement());
    let mut values = included
        .iter()
        .chain(&excluded)
        .flatten()
        .cloned()
        .collect::<Vec<_>>();
    for intervals in [&versions, &versions.complement()] {
        if let Some((Bound::Included(version), _)) = intervals.bounding_range() {
            values.push(ArcStr::from(version.to_string()));
        }
    }
    for value in values {
        for op in [Equal, NotEqual] {
            if self::range(op, &value).as_ref() == Some(range) {
                return Some(vec![vec![(op, value)]]);
            }
        }
    }

    let mut clauses = Vec::new();
    let mut missing = versions.clone();
    if let Some(points) = included {
        for value in points {
            // Inclusive comparison to an invalid version literal is raw equality,
            // even if the literal itself is the text of a wildcard specifier.
            clauses.push(vec![(GreaterEqual, value)]);
        }
    } else {
        let exceptions = excluded?;
        // Excluding a wildcard from parsed versions requires two spellings:
        // at least one inequality is true on each opaque input. Both agree on versions.
        let mut bare = Vec::new();
        let mut prefixed = Vec::new();
        let mut covered = Ranges::full();
        for (epoch, major) in excluded_prefixes(&versions.complement())? {
            let prefix = if epoch == 0 {
                major.to_string()
            } else {
                format!("{epoch}!{major}")
            };
            let value = ArcStr::from(format!("{prefix}.*"));
            covered = covered.intersection(&numeric_range(NotEqual, &value)?);
            bare.push((NotEqual, value));
            prefixed.push((NotEqual, ArcStr::from(format!("v{prefix}.*"))));
        }
        let two_spellings = !bare.is_empty();
        for value in exceptions {
            if let Some(numeric) = numeric_range(NotEqual, &value) {
                covered = covered.intersection(&numeric);
            }
            bare.push((NotEqual, value.clone()));
            prefixed.push((NotEqual, value));
        }
        clauses.push(bare);
        if two_spellings {
            clauses.push(prefixed);
        }
        missing = versions.intersection(&covered.complement());
    }
    for (low, high) in missing.iter() {
        if matches!((low, high), (Bound::Unbounded, Bound::Unbounded)) {
            clauses.push(vec![(GreaterEqual, arcstr::literal!("0.dev0"))]);
        } else {
            let upper = numeric_bound(high, false);
            for lower in numeric_bound(low, true) {
                for upper in &upper {
                    clauses.push(lower.iter().chain(upper).cloned().collect());
                }
            }
        }
    }
    Some(clauses)
}

/// A clause that accepts arbitrary opaque strings can exclude finitely many version
/// prefixes. A set spanning unbounded major releases has no such portable cover.
fn excluded_prefixes(holes: &Ranges<Version>) -> Option<std::collections::BTreeSet<(u64, u64)>> {
    let mut prefixes = std::collections::BTreeSet::new();
    for (low, high) in holes.iter() {
        let (Bound::Included(high) | Bound::Excluded(high)) = high else {
            return None;
        };
        let (epoch, first) = match low {
            Bound::Unbounded => (0, 0),
            Bound::Included(v) | Bound::Excluded(v) => (v.epoch(), v.release()[0]),
        };
        if epoch != high.epoch() {
            return None;
        }
        let last = high.release()[0];
        // A broad bounded range can have a finite but prohibitively large expansion.
        // Return an error for it rather than allocating one term per possible major.
        if last - first >= 4096 {
            return None;
        }
        prefixes.extend((first..=last).map(|major| (epoch, major)));
    }
    Some(prefixes)
}

fn numeric_bound(bound: Bound<&Version>, lower: bool) -> Dnf {
    let (version, inclusive) = match bound {
        Bound::Included(v) => (v, true),
        Bound::Excluded(v) => (v, false),
        Bound::Unbounded => return vec![vec![]],
    };
    // The cut beyond all posts of a final release has no next base release.
    // Its greatest public post number still admits arbitrarily many local labels;
    // PEP 440 equality includes all those labels.
    let base = version
        .only_release_at_precision(version.release().len())
        .expect("nonempty release");
    if *version == base.clone().with_max(Some(0)) {
        let last = base.with_post(Some(u64::MAX - 1));
        let literal = ArcStr::from(last.to_string());
        return if lower {
            vec![vec![
                (MarkerOperator::GreaterThan, literal.clone()),
                (MarkerOperator::NotEqual, literal),
            ]]
        } else {
            vec![
                vec![(MarkerOperator::LessEqual, literal.clone())],
                vec![(MarkerOperator::Equal, literal)],
            ]
        };
    }
    let operator = match (lower, inclusive) {
        (true, true) => MarkerOperator::GreaterEqual,
        (true, false) => MarkerOperator::GreaterThan,
        (false, true) => MarkerOperator::LessEqual,
        (false, false) => MarkerOperator::LessThan,
    };
    vec![vec![(operator, ArcStr::from(version.to_string()))]]
}
