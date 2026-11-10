//! Translate the two projections of an environment marker into public comparisons.

use std::ops::Bound;
use std::sync::LazyLock;

use arcstr::ArcStr;
use uv_pep440::{Version, VersionSpecifier, canonicalize_version_ranges};
use version_ranges::Ranges;

use crate::MarkerOperator;

/// The unused interval below the first legal version represents a missing parsed value.
pub(super) fn parsed_values() -> &'static Ranges<Version> {
    static VALUES: LazyLock<Ranges<Version>> = LazyLock::new(|| {
        Ranges::higher_than(
            "0.dev0"
                .parse::<Version>()
                .expect("minimum public PEP 440 version"),
        )
    });
    &VALUES
}

/// Whether a single raw input could take both projection edges.
pub(super) fn may_share_spelling(numbers: &Ranges<Version>, strings: &Ranges<ArcStr>) -> bool {
    let accepts_opaque = numbers
        .bounding_range()
        .is_some_and(|(low, _)| low == Bound::Unbounded);
    strings.iter().any(|(low, high)| {
        // No string sorts before the empty string.
        match high {
            Bound::Excluded(value) if value.is_empty() => return false,
            Bound::Included(value) if value.is_empty() => return accepts_opaque,
            _ => {}
        }
        if let (Bound::Included(left), Bound::Included(right)) = (low, high)
            && left == right
        {
            return left
                .parse::<Version>()
                .map_or(accepts_opaque, |version| numbers.contains(&version));
        }
        if accepts_opaque {
            return true;
        }
        let (
            Bound::Included(low) | Bound::Excluded(low),
            Bound::Included(high) | Bound::Excluded(high),
        ) = (low, high)
        else {
            return true;
        };
        let common = low
            .bytes()
            .zip(high.bytes())
            .take_while(|(a, b)| a == b)
            .count();
        let Some(prefix) = low.get(..common) else {
            return true;
        };
        let prefix = prefix.trim_start();
        let prefix = prefix.strip_prefix(['v', 'V']).unwrap_or(prefix);
        let Some(first) = prefix.as_bytes().first() else {
            return true;
        };
        if !first.is_ascii_digit() {
            return false;
        }
        // Once the first release component ends in a dot, it cannot be an epoch.
        // All PEP 440 spellings between the endpoints have that same major version.
        let Some((major, _)) = prefix.split_once('.') else {
            return true;
        };
        let Ok(major) = major.parse::<u64>() else {
            return true;
        };
        if major >= u64::MAX - 1 {
            return true;
        }
        let allowed = Ranges::from(VersionSpecifier::equals_star_version(Version::new([major])));
        !numbers.is_disjoint(&allowed)
    })
}

/// Ordered comparisons use the order of parsed versions. In particular, a
/// post-release sorts after its final release, so every cutoff has a writable
/// complement. Equality, wildcards, and compatible releases use PEP 440 specifiers.
pub(super) fn numeric_range(operator: MarkerOperator, literal: &ArcStr) -> Option<Ranges<Version>> {
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
    Some(
        canonicalize_version_ranges(&versions)
            .unwrap_or(versions)
            .into_iter()
            .map(|(low, high)| {
                (
                    low.map(Version::without_trailing_zeros),
                    high.map(Version::without_trailing_zeros),
                )
            })
            .collect(),
    )
}

type Comparison = (MarkerOperator, ArcStr);

/// An opaque environment value cannot equal a parseable literal. Assign the
/// unused equality point to the interval on its right so cutoffs stay canonical.
pub(super) fn opaque_range(operator: MarkerOperator, literal: &ArcStr) -> Ranges<ArcStr> {
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

/// Emit the lexical projection first, then correct its numeric projection using
/// intervals that match no opaque string. Excluding the hull of the unwanted
/// numeric intervals bounds distribution when a marker contains many holes.
pub(super) fn comparisons(versions: &Ranges<Version>, opaque: &Ranges<ArcStr>) -> Dnf {
    let versions = canonicalize_version_ranges(versions).unwrap_or_else(|| versions.clone());
    if let Some(atom) = single_comparison(&versions, opaque) {
        return vec![vec![atom]];
    }
    let mut dnf = lexical_clauses(opaque);
    // Ordering against an opaque bound also consults the original spelling
    // of parsed versions. Limit those clauses to opaque inputs explicitly.
    let Some(mut numeric) = dnf.iter().try_fold(Ranges::empty(), |union, clause| {
        let term = clause
            .iter()
            .try_fold(Ranges::full(), |term, (op, literal)| {
                Some(term.intersection(&numeric_range(*op, literal)?))
            })?;
        Some(union.union(&term))
    }) else {
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
        Bound::Included(v) => Some(ArcStr::from(v.clone().without_trailing_zeros().to_string())),
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
