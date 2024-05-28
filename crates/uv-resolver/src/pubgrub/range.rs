use pubgrub::range::Range;
use pubgrub::version_set::VersionSet;
use std::borrow::Borrow;
use std::collections::Bound;
use std::fmt::{Display, Formatter};
use std::ops::{Deref, RangeBounds};

use pep440_rs::Version;

/// A [`R<Version>`] with a Python-specific `Display` implementation.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub(crate) struct PubGrubRange(Range<Version>);

impl VersionSet for PubGrubRange {
    type V = Version;

    fn empty() -> Self {
        Self(Range::empty())
    }

    fn singleton(v: Self::V) -> Self {
        Self(Range::singleton(v))
    }

    fn complement(&self) -> Self {
        Self(self.0.complement())
    }

    fn intersection(&self, other: &Self) -> Self {
        Self(self.0.intersection(&other.0))
    }

    fn contains(&self, v: &Self::V) -> bool {
        self.0.contains(v)
    }

    fn full() -> Self {
        Self(Range::full())
    }

    fn union(&self, other: &Self) -> Self {
        Self(self.0.union(&other.0))
    }

    fn is_disjoint(&self, other: &Self) -> bool {
        self.0.is_disjoint(&other.0)
    }

    fn subset_of(&self, other: &Self) -> bool {
        self.0.subset_of(&other.0)
    }
}

impl PubGrubRange {
    /// Set of all possible versions
    pub(crate) fn full() -> Self {
        Self(Range::full())
    }

    /// Set of all versions higher or equal to some version
    pub(crate) fn higher_than(v: impl Into<Version>) -> Self {
        Self(Range::higher_than(v))
    }

    /// Set of all versions higher to some version
    pub(crate) fn strictly_higher_than(v: impl Into<Version>) -> Self {
        Self(Range::strictly_higher_than(v))
    }

    /// Set of all versions lower to some version
    pub(crate) fn strictly_lower_than(v: impl Into<Version>) -> Self {
        Self(Range::strictly_lower_than(v))
    }

    /// Set of all versions lower or equal to some version
    pub(crate) fn lower_than(v: impl Into<Version>) -> Self {
        Self(Range::lower_than(v))
    }

    pub(crate) fn complement(&self) -> Self {
        Self(self.0.complement())
    }

    pub(crate) fn simplify<'s, I, BV>(&self, versions: I) -> Self
    where
        I: Iterator<Item = BV> + 's,
        BV: Borrow<Version> + 's,
    {
        Self(self.0.simplify(versions))
    }

    pub(crate) fn from_range_bounds<R, IV>(bounds: R) -> Self
    where
        R: RangeBounds<IV>,
        IV: Clone + Into<Version>,
    {
        Self(Range::from_range_bounds(bounds))
    }
}

impl From<Range<Version>> for PubGrubRange {
    fn from(range: Range<Version>) -> Self {
        Self(range)
    }
}

impl From<PubGrubRange> for Range<Version> {
    fn from(range: PubGrubRange) -> Self {
        range.0
    }
}

impl Deref for PubGrubRange {
    type Target = Range<Version>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Python-specific [`Display`] implementation.
///
/// `|` is used as OR-operator instead of `,` since PEP 440 uses `,` as AND-operator. `==` is used
/// for single version specifiers instead of an empty prefix, again for PEP 440 where a specifier
/// needs an operator.
impl Display for PubGrubRange {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if self.0.is_empty() {
            write!(f, "∅")?;
        } else {
            for (idx, segment) in self.0.iter().enumerate() {
                if idx > 0 {
                    write!(f, " | ")?;
                }
                match segment {
                    (Bound::Unbounded, Bound::Unbounded) => write!(f, "*")?,
                    (Bound::Unbounded, Bound::Included(v)) => write!(f, "<={v}")?,
                    (Bound::Unbounded, Bound::Excluded(v)) => write!(f, "<{v}")?,
                    (Bound::Included(v), Bound::Unbounded) => write!(f, ">={v}")?,
                    (Bound::Included(v), Bound::Included(b)) => {
                        if v == b {
                            write!(f, "=={v}")?;
                        } else {
                            write!(f, ">={v}, <={b}")?;
                        }
                    }
                    (Bound::Included(v), Bound::Excluded(b)) => write!(f, ">={v}, <{b}")?,
                    (Bound::Excluded(v), Bound::Unbounded) => write!(f, ">{v}")?,
                    (Bound::Excluded(v), Bound::Included(b)) => write!(f, ">{v}, <={b}")?,
                    (Bound::Excluded(v), Bound::Excluded(b)) => write!(f, ">{v}, <{b}")?,
                };
            }
        }
        Ok(())
    }
}
