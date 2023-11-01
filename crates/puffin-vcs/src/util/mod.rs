pub(crate) use self::canonical_url::CanonicalUrl;
pub(crate) use self::counter::MetricsCounter;
pub(crate) use self::errors::CargoResult;
pub(crate) use self::hasher::StableHasher;
pub(crate) use self::hex::short_hash;
pub(crate) use self::into_url::IntoUrl;

mod canonical_url;
mod counter;
pub(crate) mod errors;
mod hasher;
mod hex;
mod into_url;
pub(crate) mod network;

/// Formats a number of bytes into a human readable SI-prefixed size.
/// Returns a tuple of `(quantity, units)`.
pub fn human_readable_bytes(bytes: u64) -> (f32, &'static str) {
    static UNITS: [&str; 7] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB", "EiB"];
    let bytes = bytes as f32;
    let i = ((bytes.log2() / 10.0) as usize).min(UNITS.len() - 1);
    (bytes / 1024_f32.powi(i as i32), UNITS[i])
}

pub fn truncate_with_ellipsis(s: &str, max_width: usize) -> String {
    // We should truncate at grapheme-boundary and compute character-widths,
    // yet the dependencies on unicode-segmentation and unicode-width are
    // not worth it.
    let mut chars = s.chars();
    let mut prefix = (&mut chars).take(max_width - 1).collect::<String>();
    if chars.next().is_some() {
        prefix.push('…');
    }
    prefix
}
