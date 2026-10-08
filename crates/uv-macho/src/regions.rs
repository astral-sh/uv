use std::collections::BTreeMap;
use std::ops::Range;

use crate::Error;

/// Nonempty file regions that cannot overlap, regardless of insertion order.
#[derive(Default)]
pub(crate) struct FileRegions {
    /// Each start offset maps to the corresponding exclusive end offset.
    by_start: BTreeMap<usize, usize>,
}

impl FileRegions {
    pub(crate) fn insert(&mut self, region: Range<usize>) -> Result<(), Error> {
        if region.is_empty() {
            return Ok(());
        }

        let overlaps_previous = self
            .by_start
            .range(..=region.start)
            .next_back()
            .is_some_and(|(_, end)| *end > region.start);
        let overlaps_next = self
            .by_start
            .range(region.start..)
            .next()
            .is_some_and(|(start, _)| *start < region.end);
        if overlaps_previous || overlaps_next {
            return Err(Error::Malformed("overlapping file regions"));
        }

        self.by_start.insert(region.start, region.end);
        Ok(())
    }

    pub(crate) fn end(&self) -> Option<usize> {
        self.by_start.last_key_value().map(|(_, end)| *end)
    }
}
