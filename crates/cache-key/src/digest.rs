use crate::cache_key::{CacheKey, CacheKeyHasher};
use seahash::SeaHasher;
use std::hash::{Hash, Hasher};

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
