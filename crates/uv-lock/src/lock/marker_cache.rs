//! Reuses large marker strings borrowed from one lockfile.

use std::borrow::Cow;
use std::collections::hash_map::Entry;
use std::str::FromStr;

use rustc_hash::FxHashMap;
use serde::Deserialize;

use uv_distribution_types::SimplifiedMarkerTree;
use uv_pep508::{Pep508Error, Reporter, TracingReporter};

use super::{DependencyWire, LockWire, PackageWire};

const MIN_MARKER_BYTES: usize = 256;
const MAX_CACHE_ENTRIES: usize = 4_096;

/// Marker text parsed after deserializing the lockfile.
#[derive(Default, Deserialize)]
#[serde(transparent)]
pub(super) struct UnparsedMarker<'a>(#[serde(borrow)] Cow<'a, str>);

impl<'a> LockWire<UnparsedMarker<'a>> {
    /// Parse marker text before restoring Python bounds and validating the lock.
    pub(super) fn parse_markers(self) -> Result<LockWire, Pep508Error> {
        let mut cache = MarkerCache::default();
        Ok(LockWire {
            version: self.version,
            revision: self.revision,
            requires_python: self.requires_python,
            fork_markers: cache.parse_all(self.fork_markers)?,
            supported_environments: cache.parse_all(self.supported_environments)?,
            required_environments: cache.parse_all(self.required_environments)?,
            conflicts: self.conflicts,
            options: self.options,
            manifest: self.manifest,
            packages: self
                .packages
                .into_iter()
                .map(|package| package.parse_markers(&mut cache))
                .collect::<Result<_, _>>()?,
        })
    }
}

impl<'a> PackageWire<UnparsedMarker<'a>> {
    fn parse_markers(self, cache: &mut MarkerCache<'a>) -> Result<PackageWire, Pep508Error> {
        let fork_markers = cache.parse_all(self.fork_markers)?;
        let mut parse_dependencies = |dependencies: Vec<DependencyWire<UnparsedMarker<'a>>>| {
            dependencies
                .into_iter()
                .map(|dependency| dependency.parse_marker(cache))
                .collect::<Result<Vec<_>, _>>()
        };
        Ok(PackageWire {
            id: self.id,
            metadata: self.metadata,
            sdist: self.sdist,
            wheels: self.wheels,
            fork_markers,
            dependencies: parse_dependencies(self.dependencies)?,
            optional_dependencies: self
                .optional_dependencies
                .into_iter()
                .map(|(extra, dependencies)| Ok((extra, parse_dependencies(dependencies)?)))
                .collect::<Result<_, Pep508Error>>()?,
            default_groups: self.default_groups,
            dependency_groups: self
                .dependency_groups
                .into_iter()
                .map(|(group, dependencies)| Ok((group, parse_dependencies(dependencies)?)))
                .collect::<Result<_, Pep508Error>>()?,
            group_requires_python: self.group_requires_python,
        })
    }
}

impl<'a> DependencyWire<UnparsedMarker<'a>> {
    fn parse_marker(self, cache: &mut MarkerCache<'a>) -> Result<DependencyWire, Pep508Error> {
        Ok(DependencyWire {
            package_id: self.package_id,
            extra: self.extra,
            marker: cache.parse(self.marker.0)?,
        })
    }
}

#[derive(Default)]
struct MarkerCache<'a> {
    // Keys borrow the input, so the entry limit also bounds the cache's allocation.
    markers: FxHashMap<&'a str, SimplifiedMarkerTree>,
}

impl<'a> MarkerCache<'a> {
    /// Reuse long, warning-free markers without retaining decoded escape sequences.
    fn parse(&mut self, source: Cow<'a, str>) -> Result<SimplifiedMarkerTree, Pep508Error> {
        if source.is_empty() {
            return Ok(SimplifiedMarkerTree::default());
        }
        if source.len() < MIN_MARKER_BYTES {
            return SimplifiedMarkerTree::from_str(&source);
        }
        let source = match source {
            Cow::Borrowed(source) => source,
            Cow::Owned(source) => return SimplifiedMarkerTree::from_str(&source),
        };
        let has_room = self.markers.len() < MAX_CACHE_ENTRIES;
        let entry = match self.markers.entry(source) {
            Entry::Occupied(entry) => return Ok(*entry.get()),
            Entry::Vacant(entry) => entry,
        };
        if !has_room {
            return SimplifiedMarkerTree::from_str(source);
        }
        let mut warned = false;
        let marker = SimplifiedMarkerTree::parse(source, &mut |kind, warning| {
            warned = true;
            TracingReporter.report(kind, warning);
        })?;
        if !warned {
            entry.insert(marker);
        }
        Ok(marker)
    }

    fn parse_all(
        &mut self,
        markers: Vec<UnparsedMarker<'a>>,
    ) -> Result<Vec<SimplifiedMarkerTree>, Pep508Error> {
        markers
            .into_iter()
            .map(|marker| self.parse(marker.0))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use uv_distribution_types::SimplifiedMarkerTree;

    use super::{MAX_CACHE_ENTRIES, MarkerCache};

    #[test]
    fn repeated_markers_are_cached() {
        let source = format!("{}os_name == 'posix'", "os_name == 'posix' and ".repeat(16));
        let mut cache = MarkerCache::default();
        let expected = cache.parse(Cow::Borrowed(&source)).expect("valid marker");
        assert_eq!(cache.markers.get(source.as_str()), Some(&expected));
        assert_eq!(
            cache.parse(Cow::Borrowed(&source)).expect("valid marker"),
            expected
        );
        assert_eq!(cache.markers.len(), 1);
    }

    #[test]
    fn markers_with_warnings_are_not_cached() {
        let source = format!("{}os.name == 'posix'", "os_name == 'posix' and ".repeat(16));
        let mut cache = MarkerCache::default();
        cache.parse(Cow::Borrowed(&source)).expect("valid marker");
        cache.parse(Cow::Borrowed(&source)).expect("valid marker");
        assert!(cache.markers.is_empty());
    }

    #[test]
    fn cache_has_an_entry_limit() {
        let keys: Vec<_> = (0..MAX_CACHE_ENTRIES)
            .map(|index| index.to_string())
            .collect();
        let source = format!("{}os_name == 'posix'", "os_name == 'posix' and ".repeat(16));
        let expected: SimplifiedMarkerTree = source.parse().expect("valid marker");
        let mut cache = MarkerCache {
            markers: keys
                .iter()
                .map(|key| (key.as_str(), SimplifiedMarkerTree::default()))
                .collect(),
        };
        assert_eq!(
            cache.parse(Cow::Borrowed(&source)).expect("valid marker"),
            expected
        );
        assert_eq!(cache.markers.len(), MAX_CACHE_ENTRIES);
        assert!(!cache.markers.contains_key(source.as_str()));
    }

    #[test]
    fn owned_markers_are_not_cached() {
        let source = format!("{}os_name == 'posix'", "os_name == 'posix' and ".repeat(16));
        let expected: SimplifiedMarkerTree = source.parse().expect("valid marker");
        let mut cache = MarkerCache::default();
        assert_eq!(
            cache.parse(Cow::Owned(source)).expect("valid marker"),
            expected
        );
        assert!(cache.markers.is_empty());
    }
}
