//! macOS version parsing and representation.

use std::fmt;

/// macOS version requirement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct MacOSVersion {
    pub major: u16,
    pub minor: u16,
}

impl MacOSVersion {
    pub const fn new(major: u16, minor: u16) -> Self {
        Self { major, minor }
    }

    /// Parse a macOS version string like "10.9" or "11.0" or "14.0".
    pub fn parse(s: &str) -> Option<Self> {
        let mut parts = s.split('.');
        let major: u16 = parts.next()?.parse().ok()?;
        let minor: u16 = parts.next().and_then(|part| part.parse().ok()).unwrap_or(0);
        Some(Self::new(major, minor))
    }

    /// Parse from a packed version (used in Mach-O `LC_BUILD_VERSION` and `LC_VERSION_MIN_MACOSX`).
    ///
    /// Format: `xxxx.yy.zz` where `x` is major, `y` is minor, `z` is patch (ignored).
    #[allow(clippy::cast_possible_truncation)]
    pub const fn from_packed(packed: u32) -> Self {
        Self {
            major: ((packed >> 16) & 0xFFFF) as u16,
            minor: ((packed >> 8) & 0xFF) as u16,
        }
    }
}

impl fmt::Display for MacOSVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse() {
        assert_eq!(MacOSVersion::parse("10.9"), Some(MacOSVersion::new(10, 9)));
        assert_eq!(MacOSVersion::parse("11.0"), Some(MacOSVersion::new(11, 0)));
        assert_eq!(MacOSVersion::parse("14"), Some(MacOSVersion::new(14, 0)));
        assert_eq!(MacOSVersion::parse(""), None);
        assert_eq!(MacOSVersion::parse("abc"), None);
    }

    #[test]
    fn test_from_packed() {
        // 10.9.0 = 0x000A0900
        assert_eq!(
            MacOSVersion::from_packed(0x000A_0900),
            MacOSVersion::new(10, 9)
        );
        // 11.0.0 = 0x000B0000
        assert_eq!(
            MacOSVersion::from_packed(0x000B_0000),
            MacOSVersion::new(11, 0)
        );
        // 14.0.0 = 0x000E0000
        assert_eq!(
            MacOSVersion::from_packed(0x000E_0000),
            MacOSVersion::new(14, 0)
        );
    }

    #[test]
    fn test_display() {
        assert_eq!(MacOSVersion::new(10, 9).to_string(), "10.9");
        assert_eq!(MacOSVersion::new(14, 0).to_string(), "14.0");
    }

    #[test]
    fn test_ord() {
        assert!(MacOSVersion::new(10, 9) < MacOSVersion::new(10, 10));
        assert!(MacOSVersion::new(10, 15) < MacOSVersion::new(11, 0));
        assert!(MacOSVersion::new(11, 0) < MacOSVersion::new(14, 0));
    }
}
