use std::hash::Hasher;

use crate::cache_key::{CacheKey, CacheKeyHasher};

/// Compute a hex string hash of a `CacheKey` object.
///
/// The value returned by [`digest`] should be stable across releases and platforms.
pub fn digest<H: CacheKey>(hashable: &H) -> String {
    to_hex(cache_key_u64(hashable))
}

/// Convert a u64 to a hex string.
fn to_hex(num: u64) -> String {
    hex::encode(num.to_le_bytes())
}

/// Compute a u64 hash of a [`CacheKey`] object.
fn cache_key_u64<H: CacheKey>(hashable: &H) -> u64 {
    let mut hasher = CacheKeyHasher::new();
    hashable.cache_key(&mut hasher);
    hasher.finish()
}
