use std::ops::Range;

/// Sort nonempty ranges by start and check for overlaps.
pub(crate) fn sort_disjoint<T: Ord>(ranges: &mut [Range<T>]) -> bool {
    ranges.sort_unstable_by(|left, right| left.start.cmp(&right.start));
    ranges.windows(2).all(|pair| pair[0].end <= pair[1].start)
}
