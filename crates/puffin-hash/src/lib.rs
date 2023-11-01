pub use canonical_url::CanonicalUrl;

mod canonical_url;

use seahash::SeaHasher;
use std::hash::{Hash, Hasher};

/// Compute a hex string hash of a [`Hashable`] object.
///
/// The hash returned by [`short_hash`] should be stable across releases and platforms.
pub fn short_hash<H: Hash>(hashable: &H) -> String {
    to_hex(hash_u64(hashable))
}

/// Convert a u64 to a hex string.
fn to_hex(num: u64) -> String {
    hex::encode(num.to_le_bytes())
}

/// Compute a u64 hash of a [`Hashable`] object.
fn hash_u64<H: Hash>(hashable: H) -> u64 {
    let mut hasher = SeaHasher::new();
    hashable.hash(&mut hasher);
    hasher.finish()
}
