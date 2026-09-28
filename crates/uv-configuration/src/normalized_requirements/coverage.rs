//! Track overlapping version exclusions while removing redundant specifiers.

use std::ops::Bound;

use uv_pep440::Version;
use version_ranges::Ranges;

/// The number of remaining specifiers that exclude each part of the version range.
pub(super) struct VersionRangeCoverage {
    endpoints: Vec<Version>,
    counts: RangeCounts,
}

impl VersionRangeCoverage {
    pub(super) fn new(exclusions: &[Ranges<Version>]) -> Self {
        let mut endpoints = Vec::new();
        for exclusion in exclusions {
            for (lower, upper) in exclusion.iter() {
                if let Bound::Included(version) | Bound::Excluded(version) = lower {
                    endpoints.push(version.clone());
                }
                if let Bound::Included(version) | Bound::Excluded(version) = upper {
                    endpoints.push(version.clone());
                }
            }
        }
        endpoints.sort();
        endpoints.dedup();

        // Each endpoint and each open interval between endpoints is a distinct coordinate.
        let counts = RangeCounts::new(2 * endpoints.len() + 1);
        let mut coverage = Self { endpoints, counts };
        for exclusion in exclusions {
            coverage.update(exclusion, 1);
        }
        coverage
    }

    /// Whether every excluded version is also excluded by another specifier.
    pub(super) fn is_redundant(&self, exclusion: &Ranges<Version>) -> bool {
        exclusion.iter().all(|bounds| {
            let (start, end) = self.coordinates(bounds);
            start >= end || self.counts.minimum(start, end) >= 2
        })
    }

    pub(super) fn remove(&mut self, exclusion: &Ranges<Version>) {
        self.update(exclusion, -1);
    }

    fn update(&mut self, exclusion: &Ranges<Version>, delta: isize) {
        for bounds in exclusion.iter() {
            let (start, end) = self.coordinates(bounds);
            if start < end {
                self.counts.add(start, end, delta);
            }
        }
    }

    /// Map inclusive and exclusive bounds to a half-open interval of coordinates.
    fn coordinates(&self, (lower, upper): (Bound<&Version>, Bound<&Version>)) -> (usize, usize) {
        let endpoint = |version| {
            self.endpoints
                .partition_point(|candidate| candidate < version)
        };
        let start = match lower {
            Bound::Included(version) => 2 * endpoint(version) + 1,
            Bound::Excluded(version) => 2 * endpoint(version) + 2,
            Bound::Unbounded => 0,
        };
        let end = match upper {
            Bound::Included(version) => 2 * endpoint(version) + 2,
            Bound::Excluded(version) => 2 * endpoint(version) + 1,
            Bound::Unbounded => 2 * self.endpoints.len() + 1,
        };
        (start, end)
    }
}

/// A segment tree supporting range increments and range-minimum queries.
///
/// Each node's minimum includes its own lazy update but excludes its ancestors' updates. This
/// permits updates and queries without pushing lazy values into child nodes.
struct RangeCounts {
    len: usize,
    minimum: Vec<isize>,
    lazy: Vec<isize>,
}

impl RangeCounts {
    fn new(len: usize) -> Self {
        Self {
            len,
            minimum: vec![0; 4 * len],
            lazy: vec![0; 4 * len],
        }
    }

    fn add(&mut self, start: usize, end: usize, delta: isize) {
        self.add_node(1, 0, self.len, start, end, delta);
    }

    fn add_node(
        &mut self,
        node: usize,
        left: usize,
        right: usize,
        start: usize,
        end: usize,
        delta: isize,
    ) {
        if end <= left || right <= start {
            return;
        }
        if start <= left && right <= end {
            self.minimum[node] += delta;
            self.lazy[node] += delta;
            return;
        }
        let middle = left + (right - left) / 2;
        self.add_node(node * 2, left, middle, start, end, delta);
        self.add_node(node * 2 + 1, middle, right, start, end, delta);
        self.minimum[node] =
            self.lazy[node] + self.minimum[node * 2].min(self.minimum[node * 2 + 1]);
    }

    fn minimum(&self, start: usize, end: usize) -> isize {
        self.minimum_node(1, 0, self.len, start, end, 0)
    }

    fn minimum_node(
        &self,
        node: usize,
        left: usize,
        right: usize,
        start: usize,
        end: usize,
        parent: isize,
    ) -> isize {
        if end <= left || right <= start {
            return isize::MAX;
        }
        if start <= left && right <= end {
            return parent + self.minimum[node];
        }
        let middle = left + (right - left) / 2;
        let parent = parent + self.lazy[node];
        self.minimum_node(node * 2, left, middle, start, end, parent)
            .min(self.minimum_node(node * 2 + 1, middle, right, start, end, parent))
    }
}
