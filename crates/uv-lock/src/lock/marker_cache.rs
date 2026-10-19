//! Reuses large marker strings while deserializing one lockfile.

use std::cell::RefCell;
use std::collections::hash_map::Entry;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::str::FromStr;

use rustc_hash::{FxHashMap, FxHasher};
use serde::{Deserialize, Deserializer, de};

use uv_distribution_types::SimplifiedMarkerTree;
use uv_pep508::{Reporter, TracingReporter};

const MIN_MARKER_BYTES: usize = 256;
const MAX_CACHE_BYTES: usize = 16 * 1024 * 1024;
const MAX_CACHE_ENTRIES: usize = 4_096;

thread_local! {
    // Serde does not pass state between sibling fields. Keep the cache scoped to the
    // synchronous lock deserializer, including its general TOML fallback.
    static MARKERS: RefCell<Option<MarkerCache>> = const { RefCell::new(None) };
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
    fn lookup(&mut self, source: &str) -> CacheLookup {
        let mut hasher = FxHasher::default();
        source.hash(&mut hasher);
        let fingerprint = hasher.finish();
        let has_room = self.markers.len() < MAX_CACHE_ENTRIES;
        match self.markers.entry(fingerprint) {
            Entry::Occupied(entry) => match entry.get() {
                Some((cached_source, marker)) => {
                    // Fingerprints only control admission; compare the full string
                    // before returning a cached parse, including on collisions.
                    if cached_source.as_ref() == source {
                        CacheLookup::Hit(*marker)
                    } else {
                        CacheLookup::Miss
                    }
                }
                None => {
                    if source.len() <= MAX_CACHE_BYTES - self.bytes {
                        CacheLookup::Repeated(fingerprint)
                    } else {
                        CacheLookup::Miss
                    }
                }
            },
            Entry::Vacant(entry) => {
                if has_room && source.len() <= MAX_CACHE_BYTES - self.bytes {
                    // Avoid copying large strings that occur only once. The first
                    // occurrence also uses the ordinary parser without tracking warnings.
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
            *entry = Some((source.into(), marker));
            self.bytes += source.len();
        }
    }
}

/// Restores an enclosing deserializer's cache, including when deserialization fails.
struct CacheGuard(Option<MarkerCache>);

impl Drop for CacheGuard {
    fn drop(&mut self) {
        MARKERS.with_borrow_mut(|cache| *cache = self.0.take());
    }
}

/// Scope marker reuse to one deserialization and restore any enclosing cache.
pub(super) fn with_cache<T>(deserialize: impl FnOnce() -> T) -> T {
    let _guard = CacheGuard(MARKERS.replace(Some(MarkerCache::default())));
    deserialize()
}

/// Parse a marker, reusing long expressions that produce no warnings.
pub(super) fn deserialize_marker<'de, D>(deserializer: D) -> Result<SimplifiedMarkerTree, D::Error>
where
    D: Deserializer<'de>,
{
    struct MarkerVisitor;

    impl de::Visitor<'_> for MarkerVisitor {
        type Value = SimplifiedMarkerTree;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a string")
        }

        fn visit_str<E: de::Error>(self, source: &str) -> Result<Self::Value, E> {
            if source.len() < MIN_MARKER_BYTES {
                return SimplifiedMarkerTree::from_str(source).map_err(de::Error::custom);
            }

            let lookup = MARKERS.with_borrow_mut(|cache| {
                cache
                    .as_mut()
                    .map_or(CacheLookup::Miss, |cache| cache.lookup(source))
            });
            let fingerprint = match lookup {
                CacheLookup::Hit(marker) => return Ok(marker),
                CacheLookup::Repeated(fingerprint) => fingerprint,
                CacheLookup::Miss => {
                    return SimplifiedMarkerTree::from_str(source).map_err(de::Error::custom);
                }
            };

            // Parsing can emit tracing events. Release the cache borrow first so a
            // reentrant deserializer can establish its own cache.
            let mut warned = false;
            let marker = SimplifiedMarkerTree::parse(source, &mut |kind, warning| {
                warned = true;
                TracingReporter.report(kind, warning);
            })
            .map_err(de::Error::custom)?;
            MARKERS.with_borrow_mut(|cache| {
                if !warned && let Some(cache) = cache {
                    cache.insert(fingerprint, source, marker);
                }
            });
            Ok(marker)
        }
    }

    deserializer.deserialize_str(MarkerVisitor)
}

#[derive(Deserialize)]
struct CachedMarker(#[serde(deserialize_with = "deserialize_marker")] SimplifiedMarkerTree);

/// Deserialize marker lists through the same cache as dependency edges.
pub(super) fn deserialize_markers<'de, D>(
    deserializer: D,
) -> Result<Vec<SimplifiedMarkerTree>, D::Error>
where
    D: Deserializer<'de>,
{
    Vec::<CachedMarker>::deserialize(deserializer)
        .map(|markers| markers.into_iter().map(|marker| marker.0).collect())
}

#[cfg(test)]
mod tests {
    use serde::de::value::{Error, StrDeserializer};

    use uv_distribution_types::SimplifiedMarkerTree;

    use super::{
        CacheLookup, MARKERS, MAX_CACHE_BYTES, MAX_CACHE_ENTRIES, MarkerCache, deserialize_marker,
        with_cache,
    };

    fn marker(source: &str) -> SimplifiedMarkerTree {
        deserialize_marker(StrDeserializer::<Error>::new(source)).expect("valid marker")
    }

    #[test]
    fn cache_is_scoped_to_one_deserialization() {
        let source = format!("{}os_name == 'posix'", "os_name == 'posix' and ".repeat(16));
        let expected = marker(&source);
        assert!(MARKERS.with_borrow(Option::is_none));

        with_cache(|| {
            assert_eq!(marker(&source), expected);
            MARKERS.with_borrow(|cache| {
                assert_eq!(cache.as_ref().expect("active marker cache").bytes, 0);
            });
            assert_eq!(marker(&source), expected);
            MARKERS.with_borrow(|cache| {
                let cache = cache.as_ref().expect("active marker cache");
                assert_eq!(cache.markers.len(), 1);
                assert_eq!(cache.bytes, source.len());
            });

            let result: Result<(), ()> = with_cache(|| {
                MARKERS.with_borrow(|cache| {
                    assert!(
                        cache
                            .as_ref()
                            .expect("nested marker cache")
                            .markers
                            .is_empty()
                    );
                });
                Err(())
            });
            assert!(result.is_err());
            assert_eq!(marker(&source), expected);
            MARKERS.with_borrow(|cache| {
                assert_eq!(
                    cache.as_ref().expect("restored marker cache").markers.len(),
                    1
                );
            });
        });

        assert!(MARKERS.with_borrow(Option::is_none));
    }

    #[test]
    fn markers_with_warnings_are_not_cached() {
        let source = format!("{}os.name == 'posix'", "os_name == 'posix' and ".repeat(16));
        with_cache(|| {
            marker(&source);
            marker(&source);
            MARKERS.with_borrow(|cache| {
                assert!(
                    cache
                        .as_ref()
                        .expect("active marker cache")
                        .markers
                        .values()
                        .all(Option::is_none)
                );
            });
        });
    }

    #[test]
    fn cache_has_byte_and_entry_limits() {
        let mut cache = MarkerCache::default();
        let marker = SimplifiedMarkerTree::default();
        let source = "x".repeat(MAX_CACHE_BYTES);
        assert_eq!(cache.lookup(&source), CacheLookup::Miss);
        let fingerprint = *cache.markers.keys().next().expect("observed marker");
        assert_eq!(cache.lookup(&source), CacheLookup::Repeated(fingerprint));
        cache.insert(fingerprint, &source, marker);
        cache.insert(fingerprint, &source, marker);
        cache.lookup("overflow");
        assert_eq!(cache.markers.len(), 1);
        assert_eq!(cache.bytes, MAX_CACHE_BYTES);

        let mut cache = MarkerCache::default();
        for index in 0..=MAX_CACHE_ENTRIES {
            cache.lookup(&format!("marker-{index}"));
        }
        assert_eq!(cache.markers.len(), MAX_CACHE_ENTRIES);
        assert_eq!(cache.bytes, 0);
    }

    #[test]
    fn fingerprint_collisions_do_not_reuse_a_different_marker() {
        let mut cache = MarkerCache::default();
        let source = "os_name == 'posix'";
        assert_eq!(cache.lookup(source), CacheLookup::Miss);
        let fingerprint = *cache.markers.keys().next().expect("observed marker");
        assert_eq!(cache.lookup(source), CacheLookup::Repeated(fingerprint));
        cache.insert(
            fingerprint,
            "sys_platform == 'win32'",
            SimplifiedMarkerTree::default(),
        );
        assert_eq!(cache.lookup(source), CacheLookup::Miss);
    }
}
