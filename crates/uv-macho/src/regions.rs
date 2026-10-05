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

        // Stored regions are disjoint, so only the last one starting before this end can overlap.
        if self
            .by_start
            .range(..region.end)
            .next_back()
            .is_some_and(|(_, end)| *end > region.start)
        {
            return Err(Error::Malformed("overlapping file regions"));
        }

        self.by_start.insert(region.start, region.end);
        Ok(())
    }

    pub(crate) fn end(&self) -> Option<usize> {
        self.by_start.last_key_value().map(|(_, end)| *end)
    }
}
