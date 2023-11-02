//! Git support is derived from Cargo's implementation.
//! Cargo is dual-licensed under either Apache 2.0 or MIT, at the user's choice.
//! Source: <https://github.com/rust-lang/cargo/blob/23eb492cf920ce051abfc56bbaf838514dc8365c/src/cargo/util/mod.rs>
pub(crate) mod errors;
pub(crate) mod retry;

pub(crate) fn truncate_with_ellipsis(s: &str, max_width: usize) -> String {
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
