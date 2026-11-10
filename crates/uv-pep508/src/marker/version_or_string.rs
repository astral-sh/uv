//! Emit PEP 508 comparisons which cover both parsed versions and opaque strings.
pub(super) mod spelling;

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
/// post-release sorts after its final release, so every cutoff has a writable
/// complement. Equality, wildcards, and compatible releases use PEP 440 specifiers.
fn numeric_range(operator: MarkerOperator, literal: &ArcStr) -> Option<Ranges<Version>> {
    let versions = match operator {
        MarkerOperator::GreaterThan
        | MarkerOperator::GreaterEqual
        | MarkerOperator::LessThan
        | MarkerOperator::LessEqual => operator.ordered_range(literal.parse::<Version>().ok()?),
        MarkerOperator::Equal | MarkerOperator::NotEqual | MarkerOperator::TildeEqual => {
            let Ok(pattern) = literal.parse() else {
                // No parsed version can equal an opaque literal.
                return Some(if operator == MarkerOperator::NotEqual {
                    Ranges::full()
                } else {
                    Ranges::empty()
                });
            };
            let specifier =
                VersionSpecifier::from_pattern(operator.to_pep440_operator()?, pattern).ok()?;
            Ranges::from(specifier)
        }
        _ => return None,
    };
    Some(canonicalize_version_ranges(&versions).unwrap_or(versions))
}

/// Opaque values sort after all versions and begin at the empty string.
const FIRST_OPAQUE: VersionOrStringValue = VersionOrStringValue::Opaque(arcstr::literal!(""));

fn map_bounds<T, U: Ord>(ranges: Ranges<T>, map: impl Fn(Bound<T>, bool) -> Bound<U>) -> Ranges<U> {
    ranges
        .into_iter()
        .map(|(low, high)| (map(low, true), map(high, false)))
        .collect()
}

pub(super) fn range(
    operator: MarkerOperator,
    literal: &ArcStr,
) -> Option<Ranges<VersionOrStringValue>> {
    // Parsed versions may share an endpoint despite different release padding.
    // Keep the original spelling only on the opaque side, where it affects ordering.
    let version = |v: Version| VersionOrStringValue::Version(v.without_trailing_zeros());
    let versions = numeric_range(operator, literal)?
        .into_iter()
        .map(|(low, high)| {
            let high = match high {
                Bound::Unbounded => Bound::Excluded(FIRST_OPAQUE),
                bound => bound.map(version),
            };
            (low.map(version), high)
        });
    let strings = opaque_range(operator, literal)
        .into_iter()
        .map(|(low, high)| {
            let low = match low {
                Bound::Unbounded => Bound::Included(FIRST_OPAQUE),
                bound => bound.map(VersionOrStringValue::Opaque),
            };
            (low, high.map(VersionOrStringValue::Opaque))
        });
    Some(versions.chain(strings).collect())
}

/// On a raw spelling range, the opaque values are exactly the strings in the
/// range. A parsed version can have multiple spellings, so its raw spelling
/// must be tested separately.
pub(super) fn spelling_range(
    raw: &Ranges<ArcStr>,
) -> (Ranges<VersionOrStringValue>, Ranges<VersionOrStringValue>) {
    let opaque = map_bounds(raw.clone(), |bound, _| {
        bound.map(VersionOrStringValue::Opaque)
    })
    .intersection(&Ranges::higher_than(FIRST_OPAQUE));
    let possible = opaque.union(&Ranges::strictly_lower_than(FIRST_OPAQUE));
    (possible, opaque)
}

/// Recover the independent version and string ranges of a decision.
fn split(range: &Ranges<VersionOrStringValue>) -> (Ranges<Version>, Ranges<ArcStr>) {
    let opaque = Ranges::higher_than(FIRST_OPAQUE);
    let versions = map_bounds(
        range.intersection(&opaque.complement()),
        |bound, _| match bound {
            Bound::Included(VersionOrStringValue::Version(v)) => Bound::Included(v),
            Bound::Excluded(VersionOrStringValue::Version(v)) => Bound::Excluded(v),
            _ => Bound::Unbounded,
        },
    );
    let strings = map_bounds(range.intersection(&opaque), |bound, lower| match bound {
        Bound::Included(VersionOrStringValue::Opaque(s)) if lower && s.is_empty() => {
            Bound::Unbounded
        }
        other => other.map(|v| {
            let VersionOrStringValue::Opaque(s) = v else {
                unreachable!()
            };
            s
        }),
    });
    (
        canonicalize_version_ranges(&versions).unwrap_or(versions),
        strings,
    )
}

/// Whether following this edge compares an opaque input against a string boundary.
pub(super) fn has_opaque_cutoff(range: &Ranges<VersionOrStringValue>) -> bool {
    range.as_singleton().is_none()
        && range
            .iter()
            .flat_map(|(lo, hi)| [lo, hi])
            .any(|bound| match bound {
                Bound::Included(VersionOrStringValue::Opaque(s))
                | Bound::Excluded(VersionOrStringValue::Opaque(s)) => s.parse::<Version>().is_ok(),
                _ => false,
            })
}

type Comparison = (MarkerOperator, ArcStr);

/// An opaque environment value cannot equal a parseable literal. Assign the
/// unused equality point to the interval on its right so cutoffs stay canonical.
fn opaque_range(operator: MarkerOperator, literal: &ArcStr) -> Ranges<ArcStr> {
    if literal.parse::<Version>().is_err() {
        return operator.ordered_range(literal.clone());
    }
    match operator {
        MarkerOperator::Equal | MarkerOperator::TildeEqual => Ranges::empty(),
        MarkerOperator::NotEqual => Ranges::full(),
        MarkerOperator::LessThan | MarkerOperator::LessEqual => {
            Ranges::strictly_lower_than(literal.clone())
        }
        MarkerOperator::GreaterThan | MarkerOperator::GreaterEqual => {
            Ranges::higher_than(literal.clone())
        }
        _ => unreachable!("containment has its own decision variable"),
    }
}

type Clause = Vec<Comparison>;
type Dnf = Vec<Clause>;

/// Lexical ordering on the subset of strings that parse as versions.
pub(super) fn parsed_spellings(operator: MarkerOperator, literal: &ArcStr) -> Ranges<ArcStr> {
    let less = matches!(
        operator,
        MarkerOperator::LessThan | MarkerOperator::LessEqual
    );
    match spelling::cut(literal) {
        spelling::Cut::BeforeAll if less => Ranges::empty(),
        spelling::Cut::AfterAll if !less => Ranges::empty(),
        spelling::Cut::BeforeAll | spelling::Cut::AfterAll => Ranges::full(),
        spelling::Cut::Before(boundary) if less => Ranges::strictly_lower_than(boundary),
        spelling::Cut::Before(boundary) => Ranges::higher_than(boundary),
    }
}

/// Compute the numeric projection when the lexical cuts cancel. In particular
/// an interval entirely between two version spellings admits no version.
fn numeric_projection(dnf: &Dnf) -> Option<Ranges<Version>> {
    let mut groups = Vec::<(Ranges<Version>, Ranges<ArcStr>)>::new();
    for clause in dnf {
        let mut versions = Ranges::full();
        let mut spellings = Ranges::full();
        for (op, literal) in clause {
            if let Some(numeric) = numeric_range(*op, literal) {
                versions = versions.intersection(&numeric);
            } else {
                spellings = spellings.intersection(&parsed_spellings(*op, literal));
            }
        }
        if !versions.is_empty() && !spellings.is_empty() {
            if let Some((_, seen)) = groups.iter_mut().find(|(range, _)| *range == versions) {
                *seen = seen.union(&spellings);
            } else {
                groups.push((versions, spellings));
            }
        }
    }
    groups
        .into_iter()
        .try_fold(Ranges::empty(), |result, (versions, spellings)| {
            (spellings == Ranges::full()).then(|| result.union(&versions))
        })
}

/// Emit the lexical projection first, then correct its numeric projection using
/// intervals that match no opaque string. Excluding the hull of the unwanted
/// numeric intervals bounds distribution when a marker contains many holes.
pub(super) fn comparisons(range: &Ranges<VersionOrStringValue>) -> Dnf {
    let (versions, opaque) = split(range);
    if let Some(atom) = single_comparison(&versions, &opaque) {
        return vec![vec![atom]];
    }
    let mut dnf = lexical_clauses(&opaque);
    // Ordering against an opaque bound also consults the original spelling
    // of parsed versions. Limit those clauses to opaque inputs explicitly.
    let Some(mut numeric) = numeric_projection(&dnf) else {
        let opaque_only = negate(&numeric_interval(Bound::Unbounded, Bound::Unbounded));
        let mut result = product(&dnf, &opaque_only);
        for (low, high) in versions.iter() {
            result.extend(numeric_interval(low, high));
        }
        return result;
    };
    let excess = numeric.intersection(&versions.complement());
    if let Some((low, high)) = excess.bounding_range() {
        let hull = Ranges::from_range_bounds((low.cloned(), high.cloned()));
        dnf = product(&dnf, &negate(&numeric_interval(low, high)));
        numeric = numeric.intersection(&hull.complement());
    }
    for (low, high) in versions.intersection(&numeric.complement()).iter() {
        dnf.extend(numeric_interval(low, high));
    }
    dnf
}

/// The opaque interval restricts which operators and spellings can represent
/// this guard. Verify the candidate on both domains before emitting it.
fn single_comparison(versions: &Ranges<Version>, opaque: &Ranges<ArcStr>) -> Option<Comparison> {
    use MarkerOperator::{
        Equal, GreaterEqual, GreaterThan, LessEqual, LessThan, NotEqual, TildeEqual,
    };
    let find = |literal: ArcStr, candidates: &[MarkerOperator]| {
        let operator = candidates.iter().copied().find(|&op| {
            opaque_range(op, &literal) == *opaque
                && numeric_range(op, &literal).as_ref() == Some(versions)
        })?;
        Some((operator, literal))
    };
    let lower = |versions: &Ranges<Version>| match versions.bounding_range()?.0 {
        Bound::Included(v) => Some(ArcStr::from(v.to_string())),
        Bound::Unbounded => Some(arcstr::literal!("0.dev0")),
        Bound::Excluded(_) => None,
    };
    if opaque.is_empty() {
        return find(lower(versions)?, &[Equal, TildeEqual]);
    }
    if *opaque == Ranges::full() {
        return find(lower(&versions.complement())?, &[NotEqual]);
    }
    match opaque.bounding_range()? {
        (Bound::Unbounded, Bound::Excluded(s)) => find(s.clone(), &[LessThan, LessEqual]),
        (Bound::Included(s), Bound::Unbounded) => find(s.clone(), &[GreaterEqual, GreaterThan]),
        (Bound::Included(s), Bound::Included(t)) if s == t => find(s.clone(), &[Equal]),
        (Bound::Unbounded, Bound::Unbounded) => {
            find(opaque.complement().as_singleton()?.clone(), &[NotEqual])
        }
        _ => None,
    }
}

/// Join intervals separated by a single excluded wildcard. Ordering against a
/// wildcard would be a raw string comparison; `!=` keeps its version semantics.
fn lexical_clauses(opaque: &Ranges<ArcStr>) -> Dnf {
    let mut ranges = opaque.iter().peekable();
    let mut clauses = Vec::new();
    while let Some((low, mut high)) = ranges.next() {
        let mut holes = Vec::new();
        while let Bound::Excluded(end) = high
            && let Some((_, next_high)) =
                ranges.next_if(|(start, _)| *start == Bound::Excluded(end))
        {
            holes.push((MarkerOperator::NotEqual, end.clone()));
            high = next_high;
        }
        // The empty string starts the opaque domain. At this boundary the
        // raw-string cutoff is exactly equality or inequality with the empty string.
        let clause = MarkerOperator::from_bounds((low, high), |_| true)
            .map(|(op, value)| {
                let op = match op {
                    MarkerOperator::LessEqual if value.is_empty() => MarkerOperator::Equal,
                    MarkerOperator::GreaterThan if value.is_empty() => MarkerOperator::NotEqual,
                    other => other,
                };
                (op, value)
            })
            .chain(holes)
            .collect();
        clauses.push(clause);
    }
    clauses
}

fn projection<T: Ord + Clone>(
    dnf: &Dnf,
    atom_range: impl Fn(MarkerOperator, &ArcStr) -> Ranges<T>,
) -> Ranges<T> {
    dnf.iter().fold(Ranges::empty(), |covered, clause| {
        let term = clause
            .iter()
            .fold(Ranges::full(), |term, (operator, literal)| {
                term.intersection(&atom_range(*operator, literal))
            });
        covered.union(&term)
    })
}

// At the ceiling of the parser's numeric domain a compatible or wildcard
// range may extend to infinity. Two different spellings of the terminal
// prefix match the same versions but cannot both equal an opaque string.
static LAST_PREFIX: std::sync::LazyLock<(Version, ArcStr, ArcStr)> =
    std::sync::LazyLock::new(|| {
        let last = u64::MAX - 1;
        let base = format!("{last}!{last}");
        (
            format!("{base}.dev0")
                .parse()
                .expect("terminal prefix floor"),
            ArcStr::from(format!("{base}.*")),
            ArcStr::from(format!("v{base}.*")),
        )
    });

/// Two cutoffs can describe a bounded interval that includes only versions. If
/// their bare spellings also overlap lexically, the PEP 440 'v' prefix puts the
/// lower bound above the digit-leading upper bound for every opaque string.
fn numeric_interval(low: Bound<&Version>, high: Bound<&Version>) -> Dnf {
    if !matches!(high, Bound::Unbounded) {
        let mut lower = match low {
            Bound::Unbounded => vec![vec![(
                MarkerOperator::GreaterEqual,
                arcstr::literal!("0.dev0"),
            )]],
            bound => numeric_bound(bound, true),
        };
        let upper = numeric_bound(high, false);
        if !projection(&lower, opaque_range).is_disjoint(&projection(&upper, opaque_range)) {
            for (_, value) in lower.iter_mut().flatten() {
                *value = ArcStr::from(format!("v{value}"));
            }
        }
        return product(&lower, &upper);
    }
    let (floor, bare, prefixed) = &*LAST_PREFIX;
    let mut tail = vec![vec![
        (MarkerOperator::Equal, bare.clone()),
        (MarkerOperator::Equal, prefixed.clone()),
    ]];
    let lower_before_floor = match low {
        Bound::Included(v) | Bound::Excluded(v) => v < floor,
        Bound::Unbounded => true,
    };
    if lower_before_floor {
        tail.extend(numeric_interval(low, Bound::Excluded(floor)));
    } else {
        tail = product(&tail, &numeric_bound(low, true));
    }
    tail
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

fn product(left: &Dnf, right: &Dnf) -> Dnf {
    let mut output = Vec::new();
    for lhs in left {
        for rhs in right {
            let mut clause = lhs.clone();
            for atom in rhs {
                if !clause.contains(atom) {
                    clause.push(atom.clone());
                }
            }
            output.push(clause);
        }
    }
    output
}

fn negate(dnf: &Dnf) -> Dnf {
    dnf.iter().fold(vec![vec![]], |terms, clause| {
        let negated = clause
            .iter()
            .map(|(operator, s)| {
                let inverse = match operator {
                    MarkerOperator::Equal => MarkerOperator::NotEqual,
                    MarkerOperator::NotEqual => MarkerOperator::Equal,
                    MarkerOperator::GreaterThan => MarkerOperator::LessEqual,
                    MarkerOperator::GreaterEqual => MarkerOperator::LessThan,
                    MarkerOperator::LessThan => MarkerOperator::GreaterEqual,
                    MarkerOperator::LessEqual => MarkerOperator::GreaterThan,
                    _ => unreachable!("numeric interval emitted an unexpected comparison"),
                };
                vec![(inverse, s.clone())]
            })
            .collect();
        product(&terms, &negated)
    })
}
