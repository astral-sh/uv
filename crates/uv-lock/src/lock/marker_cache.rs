//! Reuses large marker strings while reading one lockfile.

use std::borrow::Cow;
use std::collections::hash_map::Entry;
use std::hash::BuildHasher;
use std::str::FromStr;

use rustc_hash::{FxBuildHasher, FxHashMap};
use serde::Deserialize;

use uv_distribution_types::SimplifiedMarkerTree;
use uv_pep508::{Pep508Error, Reporter, TracingReporter};

use super::{DependencyWire, LockWire, PackageWire};

const MIN_MARKER_BYTES: usize = 256;
const MAX_CACHE_BYTES: usize = 16 * 1024 * 1024;
const MAX_CACHE_ENTRIES: usize = 4_096;

/// Marker text parsed after deserializing the lockfile.
#[derive(Default, Deserialize)]
#[serde(transparent)]
pub(super) struct UnparsedMarker<'a>(#[serde(borrow)] Cow<'a, str>);

impl LockWire<UnparsedMarker<'_>> {
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

impl PackageWire<UnparsedMarker<'_>> {
    fn parse_markers(self, cache: &mut MarkerCache) -> Result<PackageWire, Pep508Error> {
        let fork_markers = cache.parse_all(self.fork_markers)?;
        let mut parse_dependencies = |dependencies: Vec<DependencyWire<UnparsedMarker<'_>>>| {
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

impl DependencyWire<UnparsedMarker<'_>> {
    fn parse_marker(self, cache: &mut MarkerCache) -> Result<DependencyWire, Pep508Error> {
        Ok(DependencyWire {
            package_id: self.package_id,
            extra: self.extra,
            marker: cache.parse(&self.marker.0)?,
        })
    }
}

#[derive(Default)]
struct MarkerCache {
    // A fingerprint without a value records the first occurrence without retaining its text.
    markers: FxHashMap<u64, Option<(Box<str>, SimplifiedMarkerTree)>>,
    bytes: usize,
}

#[derive(Debug, PartialEq, Eq)]
enum CacheLookup {
    Hit(SimplifiedMarkerTree),
    Repeated(u64),
    Miss,
}

impl MarkerCache {
    /// Parse marker text, reusing repeated long expressions that produce no warnings.
    fn parse(&mut self, source: &str) -> Result<SimplifiedMarkerTree, Pep508Error> {
        if source.is_empty() {
            return Ok(SimplifiedMarkerTree::default());
        }
        if source.len() < MIN_MARKER_BYTES {
            return SimplifiedMarkerTree::from_str(source);
        }
        let fingerprint = match self.lookup(source) {
            CacheLookup::Hit(marker) => return Ok(marker),
            CacheLookup::Repeated(fingerprint) => fingerprint,
            CacheLookup::Miss => return SimplifiedMarkerTree::from_str(source),
        };
        let mut warned = false;
        let marker = SimplifiedMarkerTree::parse(source, &mut |kind, warning| {
            warned = true;
            TracingReporter.report(kind, warning);
        })?;
        if !warned {
            self.insert(fingerprint, source, marker);
        }
        Ok(marker)
    }

    fn parse_all(
        &mut self,
        markers: Vec<UnparsedMarker<'_>>,
    ) -> Result<Vec<SimplifiedMarkerTree>, Pep508Error> {
        markers
            .into_iter()
            .map(|marker| self.parse(&marker.0))
            .collect()
    }

    fn lookup(&mut self, source: &str) -> CacheLookup {
        let fingerprint = FxBuildHasher.hash_one(source);
        let has_room = self.markers.len() < MAX_CACHE_ENTRIES;
        match self.markers.entry(fingerprint) {
            Entry::Occupied(entry) => match entry.get() {
                // Compare the full string so fingerprint collisions cannot return another marker.
                Some((cached_source, marker)) if cached_source.as_ref() == source => {
                    CacheLookup::Hit(*marker)
                }
                None if source.len() <= MAX_CACHE_BYTES - self.bytes => {
                    CacheLookup::Repeated(fingerprint)
                }
                Some(_) | None => CacheLookup::Miss,
            },
            Entry::Vacant(entry) => {
                if has_room && source.len() <= MAX_CACHE_BYTES - self.bytes {
                    entry.insert(None);
                }
                CacheLookup::Miss
            }
        }
    }

    fn insert(&mut self, fingerprint: u64, source: &str, marker: SimplifiedMarkerTree) {
        if source.len() <= MAX_CACHE_BYTES - self.bytes
            && let Some(entry @ None) = self.markers.get_mut(&fingerprint)
        {
            *entry = Some((Box::from(source), marker));
            self.bytes += source.len();
        }
    }
}

#[cfg(test)]
mod tests {
    use uv_distribution_types::SimplifiedMarkerTree;

    use super::{CacheLookup, MAX_CACHE_BYTES, MAX_CACHE_ENTRIES, MarkerCache};

    #[test]
    fn repeated_markers_are_cached() {
        let source = format!("{}os_name == 'posix'", "os_name == 'posix' and ".repeat(16));
        let mut cache = MarkerCache::default();
        let expected = cache.parse(&source).expect("valid marker");
        assert_eq!(cache.bytes, 0);
        assert_eq!(cache.parse(&source).expect("valid marker"), expected);
        assert_eq!(cache.bytes, source.len());
        assert_eq!(cache.lookup(&source), CacheLookup::Hit(expected));
    }

    #[test]
    fn markers_with_warnings_are_not_cached() {
        let source = format!("{}os.name == 'posix'", "os_name == 'posix' and ".repeat(16));
        let mut cache = MarkerCache::default();
        cache.parse(&source).expect("valid marker");
        cache.parse(&source).expect("valid marker");
        assert_eq!(cache.bytes, 0);
    }

    #[test]
    fn cache_has_byte_and_entry_limits() {
        let mut cache = MarkerCache {
            bytes: MAX_CACHE_BYTES,
            ..MarkerCache::default()
        };
        assert_eq!(cache.lookup("os_name == 'posix'"), CacheLookup::Miss);
        assert!(cache.markers.is_empty());

        let mut cache = MarkerCache::default();
        for index in 0..=MAX_CACHE_ENTRIES {
            cache.lookup(&format!("marker-{index}"));
        }
        assert_eq!(cache.markers.len(), MAX_CACHE_ENTRIES);
        assert_eq!(cache.bytes, 0);
    }

    #[test]
    fn fingerprint_collisions_do_not_reuse_a_different_marker() {
        let source = "os_name == 'posix'";
        let mut cache = MarkerCache::default();
        cache.lookup(source);
        let fingerprint = *cache.markers.keys().next().expect("observed marker");
        cache.insert(
            fingerprint,
            "sys_platform == 'win32'",
            SimplifiedMarkerTree::default(),
        );
        assert_eq!(cache.lookup(source), CacheLookup::Miss);
    }
}
