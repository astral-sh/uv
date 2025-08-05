//! Platform detection for operating system, architecture, and libc.

use std::cmp;
use std::fmt;
use std::str::FromStr;
use thiserror::Error;

pub use crate::arch::{Arch, ArchVariant};
pub use crate::libc::{Libc, LibcDetectionError, LibcVersion};
pub use crate::os::Os;

mod arch;
mod cpuinfo;
mod libc;
mod os;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Unknown operating system: {0}")]
    UnknownOs(String),
    #[error("Unknown architecture: {0}")]
    UnknownArch(String),
    #[error("Unknown libc environment: {0}")]
    UnknownLibc(String),
    #[error("Unsupported variant `{0}` for architecture `{1}`")]
    UnsupportedVariant(String, String),
    #[error(transparent)]
    LibcDetectionError(#[from] crate::libc::LibcDetectionError),
    #[error("Invalid platform format: {0}")]
    InvalidPlatformFormat(String),
}

/// A platform identifier that combines operating system, architecture, and libc.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Platform {
    pub os: Os,
    pub arch: Arch,
    pub libc: Libc,
}

impl Platform {
    /// Create a new platform with the given components.
    pub fn new(os: Os, arch: Arch, libc: Libc) -> Self {
        Self { os, arch, libc }
    }

    /// Detect the platform from the current environment.
    pub fn from_env() -> Result<Self, Error> {
        let os = Os::from_env();
        let arch = Arch::from_env();
        let libc = Libc::from_env()?;
        Ok(Self { os, arch, libc })
    }

    /// Check if this platform supports running another platform.
    pub fn supports(&self, other: &Self) -> bool {
        // If platforms are exactly equal, they're compatible
        if self == other {
            return true;
        }

        // OS must match exactly
        if self.os != other.os {
            return false;
        }

        // Libc must match exactly
        if self.libc != other.libc {
            return false;
        }

        // Check architecture support
        // This includes transparent emulation (e.g., x86_64 on ARM64 Windows/macOS)
        if self.arch.supports(other.arch) {
            return true;
        }

        // TODO(zanieb): Allow inequal variants, as `Arch::supports` does not
        // implement this yet. See https://github.com/astral-sh/uv/pull/9788
        // For now, allow same architecture family as a fallback
        self.arch.family() == other.arch.family()
    }
}

impl fmt::Display for Platform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}-{}-{}", self.os, self.arch, self.libc)
    }
}

impl FromStr for Platform {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.split('-').collect();

        if parts.len() < 3 {
            return Err(Error::InvalidPlatformFormat(format!(
                "expected at least 3 parts separated by '-', got {}",
                parts.len()
            )));
        }

        // Handle potential arch variants (e.g., x86_64_v3)
        let os = Os::from_str(parts[0])?;

        // Join middle parts for arch in case it contains dashes (like arch variants)
        let arch_parts = &parts[1..parts.len() - 1];
        let arch_str = arch_parts.join("-");
        let arch = Arch::from_str(&arch_str)?;

        let libc = Libc::from_str(parts[parts.len() - 1])?;

        Ok(Self { os, arch, libc })
    }
}

impl Ord for Platform {
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        self.os
            .to_string()
            .cmp(&other.os.to_string())
            // Then architecture
            .then_with(|| {
                if self.arch.family == other.arch.family {
                    return self.arch.variant.cmp(&other.arch.variant);
                }

                // For the time being, manually make aarch64 windows disfavored on its own host
                // platform, because most packages don't have wheels for aarch64 windows, making
                // emulation more useful than native execution!
                //
                // The reason we do this in "sorting" and not "supports" is so that we don't
                // *refuse* to use an aarch64 windows pythons if they happen to be installed and
                // nothing else is available.
                //
                // Similarly if someone manually requests an aarch64 windows install, we should
                // respect that request (this is the way users should "override" this behaviour).
                let preferred = if self.os.is_windows() {
                    Arch {
                        family: target_lexicon::Architecture::X86_64,
                        variant: None,
                    }
                } else {
                    // Prefer native architectures
                    Arch::from_env()
                };

                match (
                    self.arch.family == preferred.family,
                    other.arch.family == preferred.family,
                ) {
                    (true, true) => unreachable!(),
                    (true, false) => cmp::Ordering::Greater,
                    (false, true) => cmp::Ordering::Less,
                    (false, false) => {
                        // Both non-preferred, fallback to lexicographic order
                        self.arch
                            .family
                            .to_string()
                            .cmp(&other.arch.family.to_string())
                    }
                }
            })
            // Finally compare libc
            .then_with(|| self.libc.to_string().cmp(&other.libc.to_string()))
    }
}

impl PartialOrd for Platform {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_platform_display() {
        let platform = Platform {
            os: Os::from_str("linux").unwrap(),
            arch: Arch::from_str("x86_64").unwrap(),
            libc: Libc::from_str("gnu").unwrap(),
        };
        assert_eq!(platform.to_string(), "linux-x86_64-gnu");
    }

    #[test]
    fn test_platform_from_str() {
        let platform = Platform::from_str("macos-aarch64-none").unwrap();
        assert_eq!(platform.os.to_string(), "macos");
        assert_eq!(platform.arch.to_string(), "aarch64");
        assert_eq!(platform.libc.to_string(), "none");
    }

    #[test]
    fn test_platform_from_str_with_arch_variant() {
        let platform = Platform::from_str("linux-x86_64_v3-gnu").unwrap();
        assert_eq!(platform.os.to_string(), "linux");
        assert_eq!(platform.arch.to_string(), "x86_64_v3");
        assert_eq!(platform.libc.to_string(), "gnu");
    }

    #[test]
    fn test_platform_from_str_error() {
        assert!(Platform::from_str("linux-x86_64").is_err());
        assert!(Platform::from_str("invalid").is_err());
    }

    #[test]
    fn test_platform_sorting() {
        let p1 = Platform::from_str("linux-x86_64-gnu").unwrap();
        let p2 = Platform::from_str("linux-aarch64-gnu").unwrap();
        let p3 = Platform::from_str("macos-x86_64-none").unwrap();
        let p4 = Platform::from_str("linux-x86_64-musl").unwrap();

        // OS sorting takes precedence
        assert!(p1 < p3);
        assert!(p2 < p3);

        // Same OS, architecture comparison
        // This will depend on the current architecture
        let _ = p1.cmp(&p2);

        // Same OS and arch, libc comparison
        assert!(p1 < p4); // "gnu" < "musl" lexicographically
    }

    #[test]
    fn test_platform_supports() {
        let native = Platform::from_str("linux-x86_64-gnu").unwrap();
        let same = Platform::from_str("linux-x86_64-gnu").unwrap();
        let different_arch = Platform::from_str("linux-aarch64-gnu").unwrap();
        let different_os = Platform::from_str("macos-x86_64-none").unwrap();
        let different_libc = Platform::from_str("linux-x86_64-musl").unwrap();

        // Exact match
        assert!(native.supports(&same));

        // Different OS - not supported
        assert!(!native.supports(&different_os));

        // Different libc - not supported
        assert!(!native.supports(&different_libc));

        // Different architecture but same family
        // x86_64 doesn't support aarch64 even though both are 64-bit
        assert!(!native.supports(&different_arch));

        // Test architecture family support
        let x86_64_v2 = Platform::from_str("linux-x86_64_v2-gnu").unwrap();
        let x86_64_v3 = Platform::from_str("linux-x86_64_v3-gnu").unwrap();
        // These have the same architecture family (both x86_64)
        assert_eq!(native.arch.family(), x86_64_v2.arch.family());
        assert_eq!(native.arch.family(), x86_64_v3.arch.family());
        // Due to the family check, these should support each other
        assert!(native.supports(&x86_64_v2));
        assert!(native.supports(&x86_64_v3));
    }
}
