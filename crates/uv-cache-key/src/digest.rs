use std::borrow::Cow;
use std::hash::{Hash, Hasher};

use seahash::SeaHasher;

use crate::cache_key::{CacheKey, CacheKeyHasher};

/// Compute a hex string hash of a `CacheKey` object.
///
/// The value returned by [`cache_digest`] should be stable across releases and platforms.
pub fn cache_digest<H: CacheKey>(hashable: &H) -> String {
    /// Compute a u64 hash of a [`CacheKey`] object.
    fn cache_key_u64<H: CacheKey>(hashable: &H) -> u64 {
        let mut hasher = CacheKeyHasher::new();
        hashable.cache_key(&mut hasher);
        hasher.finish()
    }

    to_hex(cache_key_u64(hashable))
}

/// Compute a hex string hash of a hashable object.
pub fn hash_digest<H: Hash>(hashable: &H) -> String {
    /// Compute a u64 hash of a hashable object.
    fn hash_u64<H: Hash>(hashable: &H) -> u64 {
        let mut hasher = SeaHasher::new();
        hashable.hash(&mut hasher);
        hasher.finish()
    }

    to_hex(hash_u64(hashable))
}

/// Convert a u64 to a hex string.
fn to_hex(num: u64) -> String {
    hex::encode(num.to_le_bytes())
}

/// Normalize a name for use in a cache entry.
///
/// Replaces non-alphanumeric characters with dashes, and lowercases the name.
///
/// If `max_len` is provided, the output is truncated to at most that many bytes
/// (trailing dashes from truncation are stripped).
pub fn cache_name(name: &str, max_len: Option<usize>) -> Option<Cow<'_, str>> {
    let limit = max_len.unwrap_or(usize::MAX);

    if name.bytes().all(|c| matches!(c, b'0'..=b'9' | b'a'..=b'f')) {
        return if name.is_empty() {
            None
        } else {
            Some(Cow::Borrowed(name.get(..limit).unwrap_or(name)))
        };
    }
    let mut normalized = String::with_capacity(name.len().min(limit));
    let mut dash = false;
    for char in name.bytes().take(limit) {
        match char {
            b'0'..=b'9' | b'a'..=b'z' | b'A'..=b'Z' => {
                dash = false;
                normalized.push(char.to_ascii_lowercase() as char);
            }
            _ => {
                if !dash {
                    normalized.push('-');
                    dash = true;
                }
            }
        }
    }
    if normalized.ends_with('-') {
        normalized.pop();
    }
    if normalized.is_empty() {
        None
    } else {
        Some(Cow::Owned(normalized))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_name() {
        assert_eq!(cache_name("foo", None), Some("foo".into()));
        assert_eq!(cache_name("foo-bar", None), Some("foo-bar".into()));
        assert_eq!(cache_name("foo_bar", None), Some("foo-bar".into()));
        assert_eq!(cache_name("foo-bar_baz", None), Some("foo-bar-baz".into()));
        assert_eq!(cache_name("foo-bar_baz_", None), Some("foo-bar-baz".into()));
        assert_eq!(cache_name("foo-_bar_baz", None), Some("foo-bar-baz".into()));
        assert_eq!(cache_name("_+-_", None), None);
    }

    #[test]
    fn test_cache_name_max_len() {
        // Basic truncation
        let long = "a".repeat(300);
        let result = cache_name(&long, Some(100)).unwrap();
        assert_eq!(result.len(), 100);

        // Hex-only path borrows a subslice
        let long_hex = "abcdef".repeat(50);
        let result = cache_name(&long_hex, Some(100)).unwrap();
        assert_eq!(result.len(), 100);

        // Trailing dash from truncation at separator is stripped
        assert_eq!(cache_name("aaaa_bbbb", Some(5)), Some("aaaa".into()));

        // None means no limit
        assert_eq!(cache_name(&long, None).unwrap().len(), 300);
    }
}
