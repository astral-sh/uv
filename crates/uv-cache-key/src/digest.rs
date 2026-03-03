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
pub fn cache_name(name: &str) -> Option<Cow<'_, str>> {
    if name.bytes().all(|c| matches!(c, b'0'..=b'9' | b'a'..=b'f')) {
        return if name.is_empty() {
            None
        } else {
            Some(Cow::Borrowed(name))
        };
    }
    let mut normalized = String::with_capacity(name.len());
    let mut dash = false;
    for char in name.bytes() {
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
        assert_eq!(cache_name("foo"), Some("foo".into()));
        assert_eq!(cache_name("foo-bar"), Some("foo-bar".into()));
        assert_eq!(cache_name("foo_bar"), Some("foo-bar".into()));
        assert_eq!(cache_name("foo-bar_baz"), Some("foo-bar-baz".into()));
        assert_eq!(cache_name("foo-bar_baz_"), Some("foo-bar-baz".into()));
        assert_eq!(cache_name("foo-_bar_baz"), Some("foo-bar-baz".into()));
        assert_eq!(cache_name("_+-_"), None);
    }
}
