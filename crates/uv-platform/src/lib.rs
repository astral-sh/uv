//! Platform detection for operating system, architecture, and libc.

use std::cmp;
use std::fmt;
use std::str::FromStr;
use target_lexicon::Architecture;
use thiserror::Error;
use tracing::trace;

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

    /// Create a platform from string parts (os, arch, libc).
    pub fn from_parts(os: &str, arch: &str, libc: &str) -> Result<Self, Error> {
        Ok(Self {
            os: Os::from_str(os)?,
            arch: Arch::from_str(arch)?,
            libc: Libc::from_str(libc)?,
        })
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

        if !self.os.supports(other.os) {
            trace!(
                "Operating system `{}` is not compatible with `{}`",
                self.os, other.os
            );
            return false;
        }

        // Libc must match exactly, unless we're on emscripten — in which case it doesn't matter
        if self.libc != other.libc && !(other.os.is_emscripten() || self.os.is_emscripten()) {
            trace!(
                "Libc `{}` is not compatible with `{}`",
                self.libc, other.libc
            );
            return false;
        }

        // Check architecture compatibility
        if self.arch == other.arch {
            return true;
        }

        #[allow(clippy::unnested_or_patterns)]
        if self.os.is_windows()
            && matches!(
                (self.arch.family(), other.arch.family()),
                // 32-bit x86 binaries work fine on 64-bit x86 windows
                (Architecture::X86_64, Architecture::X86_32(_)) |
                // Both 32-bit and 64-bit binaries are transparently emulated on aarch64 windows
                (Architecture::Aarch64(_), Architecture::X86_64) |
                (Architecture::Aarch64(_), Architecture::X86_32(_))
            )
        {
            return true;
        }

        if self.os.is_macos()
            && matches!(
                (self.arch.family(), other.arch.family()),
                // macOS aarch64 runs emulated x86_64 binaries transparently if you have Rosetta
                // installed. We don't try to be clever and check if that's the case here,
                // we just assume that if x86_64 distributions are available, they're usable.
                (Architecture::Aarch64(_), Architecture::X86_64)
            )
        {
            return true;
        }

        // Wasm32 can run on any architecture
        if other.arch.is_wasm() {
            return true;
        }

        // TODO: Allow inequal variants, as we don't implement variant support checks yet.
        // See https://github.com/astral-sh/uv/pull/9788
        // For now, allow same architecture family as a fallback
        if self.arch.family() != other.arch.family() {
            return false;
        }

        true
    }

    /// Convert this platform to a `cargo-dist` style triple string.
    pub fn as_cargo_dist_triple(&self) -> String {
        use target_lexicon::{
            Architecture, ArmArchitecture, OperatingSystem, Riscv64Architecture, X86_32Architecture,
        };

        let Self { os, arch, libc } = &self;

        let arch_name = match arch.family() {
            // Special cases where Display doesn't match target triple
            Architecture::X86_32(X86_32Architecture::I686) => "i686".to_string(),
            Architecture::Riscv64(Riscv64Architecture::Riscv64) => "riscv64gc".to_string(),
            _ => arch.to_string(),
        };
        let vendor = match &**os {
            OperatingSystem::Darwin(_) => "apple",
            OperatingSystem::Windows => "pc",
            _ => "unknown",
        };
        let os_name = match &**os {
            OperatingSystem::Darwin(_) => "darwin",
            _ => &os.to_string(),
        };

        let abi = match (&**os, libc) {
            (OperatingSystem::Windows, _) => Some("msvc".to_string()),
            (OperatingSystem::Linux, Libc::Some(env)) => Some({
                // Special suffix for ARM with hardware float
                if matches!(arch.family(), Architecture::Arm(ArmArchitecture::Armv7)) {
                    format!("{env}eabihf")
                } else {
                    env.to_string()
                }
            }),
            _ => None,
        };

        format!(
            "{arch_name}-{vendor}-{os_name}{abi}",
            abi = abi.map(|abi| format!("-{abi}")).unwrap_or_default()
        )
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

        if parts.len() != 3 {
            return Err(Error::InvalidPlatformFormat(format!(
                "expected exactly 3 parts separated by '-', got {}",
                parts.len()
            )));
        }

        Self::from_parts(parts[0], parts[1], parts[2])
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
                    (true, false) => cmp::Ordering::Less,
                    (false, true) => cmp::Ordering::Greater,
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

impl From<&uv_platform_tags::Platform> for Platform {
    fn from(value: &uv_platform_tags::Platform) -> Self {
        Self {
            os: Os::from(value.os()),
            arch: Arch::from(&value.arch()),
            libc: Libc::from(value.os()),
        }
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
    fn test_platform_from_parts() {
        let platform = Platform::from_parts("linux", "x86_64", "gnu").unwrap();
        assert_eq!(platform.os.to_string(), "linux");
        assert_eq!(platform.arch.to_string(), "x86_64");
        assert_eq!(platform.libc.to_string(), "gnu");

        // Test with arch variant
        let platform = Platform::from_parts("linux", "x86_64_v3", "musl").unwrap();
        assert_eq!(platform.os.to_string(), "linux");
        assert_eq!(platform.arch.to_string(), "x86_64_v3");
        assert_eq!(platform.libc.to_string(), "musl");

        // Test error cases
        assert!(Platform::from_parts("invalid_os", "x86_64", "gnu").is_err());
        assert!(Platform::from_parts("linux", "invalid_arch", "gnu").is_err());
        assert!(Platform::from_parts("linux", "x86_64", "invalid_libc").is_err());
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
        // Too few parts
        assert!(Platform::from_str("linux-x86_64").is_err());
        assert!(Platform::from_str("invalid").is_err());

        // Too many parts (would have been accepted by the old code)
        assert!(Platform::from_str("linux-x86-64-gnu").is_err());
        assert!(Platform::from_str("linux-x86_64-gnu-extra").is_err());
    }

    #[test]
    fn test_platform_sorting_os_precedence() {
        let linux = Platform::from_str("linux-x86_64-gnu").unwrap();
        let macos = Platform::from_str("macos-x86_64-none").unwrap();
        let windows = Platform::from_str("windows-x86_64-none").unwrap();

        // OS sorting takes precedence (alphabetical)
        assert!(linux < macos);
        assert!(macos < windows);
    }

    #[test]
    fn test_platform_sorting_libc() {
        let gnu = Platform::from_str("linux-x86_64-gnu").unwrap();
        let musl = Platform::from_str("linux-x86_64-musl").unwrap();

        // Same OS and arch, libc comparison (alphabetical)
        assert!(gnu < musl);
    }

    #[test]
    fn test_platform_sorting_arch_linux() {
        // Test that Linux prefers the native architecture
        use crate::arch::test_support::{aarch64, run_with_arch, x86_64};

        let linux_x86_64 = Platform::from_str("linux-x86_64-gnu").unwrap();
        let linux_aarch64 = Platform::from_str("linux-aarch64-gnu").unwrap();

        // On x86_64 Linux, x86_64 should be preferred over aarch64
        run_with_arch(x86_64(), || {
            assert!(linux_x86_64 < linux_aarch64);
        });

        // On aarch64 Linux, aarch64 should be preferred over x86_64
        run_with_arch(aarch64(), || {
            assert!(linux_aarch64 < linux_x86_64);
        });
    }

    #[test]
    fn test_platform_sorting_arch_macos() {
        use crate::arch::test_support::{aarch64, run_with_arch, x86_64};

        let macos_x86_64 = Platform::from_str("macos-x86_64-none").unwrap();
        let macos_aarch64 = Platform::from_str("macos-aarch64-none").unwrap();

        // On x86_64 macOS, x86_64 should be preferred over aarch64
        run_with_arch(x86_64(), || {
            assert!(macos_x86_64 < macos_aarch64);
        });

        // On aarch64 macOS, aarch64 should be preferred over x86_64
        run_with_arch(aarch64(), || {
            assert!(macos_aarch64 < macos_x86_64);
        });
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
        // x86_64 doesn't support aarch64 on Linux
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

    #[test]
    fn test_windows_aarch64_platform_sorting() {
        // Test that on Windows, x86_64 is preferred over aarch64
        let windows_x86_64 = Platform::from_str("windows-x86_64-none").unwrap();
        let windows_aarch64 = Platform::from_str("windows-aarch64-none").unwrap();

        // x86_64 should sort before aarch64 on Windows (preferred)
        assert!(windows_x86_64 < windows_aarch64);

        // Test with multiple Windows platforms
        let mut platforms = [
            Platform::from_str("windows-aarch64-none").unwrap(),
            Platform::from_str("windows-x86_64-none").unwrap(),
            Platform::from_str("windows-x86-none").unwrap(),
        ];

        platforms.sort();

        // After sorting on Windows, the order should be: x86_64 (preferred), aarch64, x86
        // x86_64 is preferred on Windows regardless of native architecture
        assert_eq!(platforms[0].arch.to_string(), "x86_64");
        assert_eq!(platforms[1].arch.to_string(), "aarch64");
        assert_eq!(platforms[2].arch.to_string(), "x86");
    }

    #[test]
    fn test_windows_sorting_always_prefers_x86_64() {
        // Test that Windows always prefers x86_64 regardless of host architecture
        use crate::arch::test_support::{aarch64, run_with_arch, x86_64};

        let windows_x86_64 = Platform::from_str("windows-x86_64-none").unwrap();
        let windows_aarch64 = Platform::from_str("windows-aarch64-none").unwrap();

        // Even with aarch64 as host, Windows should still prefer x86_64
        run_with_arch(aarch64(), || {
            assert!(windows_x86_64 < windows_aarch64);
        });

        // With x86_64 as host, Windows should still prefer x86_64
        run_with_arch(x86_64(), || {
            assert!(windows_x86_64 < windows_aarch64);
        });
    }

    #[test]
    fn test_windows_aarch64_supports() {
        // Test that Windows aarch64 can run x86_64 binaries through emulation
        let windows_aarch64 = Platform::from_str("windows-aarch64-none").unwrap();
        let windows_x86_64 = Platform::from_str("windows-x86_64-none").unwrap();

        // aarch64 Windows supports x86_64 through transparent emulation
        assert!(windows_aarch64.supports(&windows_x86_64));

        // But x86_64 doesn't support aarch64
        assert!(!windows_x86_64.supports(&windows_aarch64));

        // Self-support should always work
        assert!(windows_aarch64.supports(&windows_aarch64));
        assert!(windows_x86_64.supports(&windows_x86_64));
    }

    #[test]
    fn test_from_platform_tags_platform() {
        // Test conversion from uv_platform_tags::Platform to uv_platform::Platform
        let tags_platform = uv_platform_tags::Platform::new(
            uv_platform_tags::Os::Windows,
            uv_platform_tags::Arch::X86_64,
        );
        let platform = Platform::from(&tags_platform);

        assert_eq!(platform.os.to_string(), "windows");
        assert_eq!(platform.arch.to_string(), "x86_64");
        assert_eq!(platform.libc.to_string(), "none");

        // Test with manylinux
        let tags_platform_linux = uv_platform_tags::Platform::new(
            uv_platform_tags::Os::Manylinux {
                major: 2,
                minor: 17,
            },
            uv_platform_tags::Arch::Aarch64,
        );
        let platform_linux = Platform::from(&tags_platform_linux);

        assert_eq!(platform_linux.os.to_string(), "linux");
        assert_eq!(platform_linux.arch.to_string(), "aarch64");
        assert_eq!(platform_linux.libc.to_string(), "gnu");
    }
}
