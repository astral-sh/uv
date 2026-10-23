//! Generated with `python3 scripts/generate-aarch64-variant-profiles.py`.
//! Architecture ancestry from archspec-json, licensed under the MIT license (LICENSE-archspec).

/// Architecture versions for a Linux CPU implementer and part, in preference order.
pub(super) fn linux_versions(implementer: &str, part: &str) -> Option<&'static [&'static str]> {
    Some(match (implementer, part) {
        ("0x43", "0x0af") => &["8.1a", "8a"],
        ("0x46", "0x001") => &["8.2a", "8.1a", "8a"],
        ("0x41", "0xd0c") => &["8.2a", "8.1a", "8a"],
        ("0x41", "0xd40") => &["8.4a", "8.3a", "8.2a", "8.1a", "8a"],
        ("0x41", "0xd49" | "0xd4f") => &["9.0a", "8.5a", "8.4a", "8.3a", "8.2a", "8.1a", "8a"],
        ("0xc0", "0xac3" | "0xac4") => &["8.6a", "8.5a", "8.4a", "8.3a", "8.2a", "8.1a", "8a"],
        ("0x41", "0xd83") => &["9.0a", "8.6a", "8.5a", "8.4a", "8.3a", "8.2a", "8.1a", "8a"],
        _ => return None,
    })
}
