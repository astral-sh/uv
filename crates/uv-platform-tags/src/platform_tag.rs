use std::fmt::Formatter;
use std::str::FromStr;

use uv_small_str::SmallString;

use crate::{Arch, BinaryFormat};

/// A tag to represent the platform compatibility of a Python distribution.
///
/// This is the third segment in the wheel filename, following the language and ABI tags. For
/// example, in `cp39-none-manylinux_2_24_x86_64.whl`, the platform tag is `manylinux_2_24_x86_64`.
///
/// For simplicity (and to reduce struct size), the non-Linux, macOS, and Windows variants (like
/// FreeBSD) store an opaque suffix, which combines the release (like `3.14`) and architecture (like
/// `x86_64`) into a single string (like `3_14_x86_64`).
#[derive(
    Debug,
    Clone,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Hash,
    rkyv::Archive,
    rkyv::Deserialize,
    rkyv::Serialize,
)]
#[rkyv(derive(Debug))]
pub enum PlatformTag {
    /// Ex) `any`
    Any,
    /// Ex) `manylinux_2_24_x86_64`
    Manylinux { major: u16, minor: u16, arch: Arch },
    /// Ex) `manylinux1_x86_64`
    Manylinux1 { arch: Arch },
    /// Ex) `manylinux2010_x86_64`
    Manylinux2010 { arch: Arch },
    /// Ex) `manylinux2014_x86_64`
    Manylinux2014 { arch: Arch },
    /// Ex) `linux_x86_64`
    Linux { arch: Arch },
    /// Ex) `musllinux_1_2_x86_64`
    Musllinux { major: u16, minor: u16, arch: Arch },
    /// Ex) `macosx_11_0_x86_64`
    Macos {
        major: u16,
        minor: u16,
        binary_format: BinaryFormat,
    },
    /// Ex) `win32`
    Win32,
    /// Ex) `win_amd64`
    WinAmd64,
    /// Ex) `win_arm64`
    WinArm64,
    /// Ex) `win_ia64`
    WinIa64,
    /// Ex) `android_21_x86_64`
    Android { api_level: u16, arch: Arch },
    /// Ex) `freebsd_12_x86_64`
    FreeBsd { release_arch: SmallString },
    /// Ex) `netbsd_9_x86_64`
    NetBsd { release_arch: SmallString },
    /// Ex) `openbsd_6_x86_64`
    OpenBsd { release_arch: SmallString },
    /// Ex) `dragonfly_6_x86_64`
    Dragonfly { release_arch: SmallString },
    /// Ex) `haiku_1_x86_64`
    Haiku { release_arch: SmallString },
    /// Ex) `illumos_5_11_x86_64`
    Illumos { release_arch: SmallString },
    /// Ex) `solaris_11_4_x86_64`
    Solaris { release_arch: SmallString },
}

impl PlatformTag {
    /// Return a pretty string representation of the language tag.
    pub fn pretty(&self) -> Option<&'static str> {
        match self {
            PlatformTag::Any => None,
            PlatformTag::Manylinux { .. } => Some("Linux"),
            PlatformTag::Manylinux1 { .. } => Some("Linux"),
            PlatformTag::Manylinux2010 { .. } => Some("Linux"),
            PlatformTag::Manylinux2014 { .. } => Some("Linux"),
            PlatformTag::Linux { .. } => Some("Linux"),
            PlatformTag::Musllinux { .. } => Some("Linux"),
            PlatformTag::Macos { .. } => Some("macOS"),
            PlatformTag::Win32 => Some("Windows"),
            PlatformTag::WinAmd64 => Some("Windows"),
            PlatformTag::WinArm64 => Some("Windows"),
            PlatformTag::WinIa64 => Some("Windows"),
            PlatformTag::Android { .. } => Some("Android"),
            PlatformTag::FreeBsd { .. } => Some("FreeBSD"),
            PlatformTag::NetBsd { .. } => Some("NetBSD"),
            PlatformTag::OpenBsd { .. } => Some("OpenBSD"),
            PlatformTag::Dragonfly { .. } => Some("DragonFly"),
            PlatformTag::Haiku { .. } => Some("Haiku"),
            PlatformTag::Illumos { .. } => Some("Illumos"),
            PlatformTag::Solaris { .. } => Some("Solaris"),
        }
    }
}

impl PlatformTag {
    /// Returns `true` if the platform is manylinux-only.
    pub fn is_manylinux(&self) -> bool {
        matches!(
            self,
            Self::Manylinux { .. }
                | Self::Manylinux1 { .. }
                | Self::Manylinux2010 { .. }
                | Self::Manylinux2014 { .. }
        )
    }

    /// Returns `true` if the platform is Linux-only.
    pub fn is_linux(&self) -> bool {
        matches!(
            self,
            Self::Manylinux { .. }
                | Self::Manylinux1 { .. }
                | Self::Manylinux2010 { .. }
                | Self::Manylinux2014 { .. }
                | Self::Musllinux { .. }
                | Self::Linux { .. }
        )
    }

    /// Returns `true` if the platform is macOS-only.
    pub fn is_macos(&self) -> bool {
        matches!(self, Self::Macos { .. })
    }

    /// Returns `true` if the platform is Windows-only.
    pub fn is_windows(&self) -> bool {
        matches!(
            self,
            Self::Win32 | Self::WinAmd64 | Self::WinArm64 | Self::WinIa64
        )
    }

    /// Returns `true` if the tag is only applicable on ARM platforms.
    pub fn is_arm(&self) -> bool {
        matches!(
            self,
            Self::Manylinux {
                arch: Arch::Aarch64,
                ..
            } | Self::Manylinux1 {
                arch: Arch::Aarch64,
                ..
            } | Self::Manylinux2010 {
                arch: Arch::Aarch64,
                ..
            } | Self::Manylinux2014 {
                arch: Arch::Aarch64,
                ..
            } | Self::Linux {
                arch: Arch::Aarch64,
                ..
            } | Self::Musllinux {
                arch: Arch::Aarch64,
                ..
            } | Self::Macos {
                binary_format: BinaryFormat::Arm64,
                ..
            } | Self::WinArm64
                | Self::Android {
                    arch: Arch::Aarch64,
                    ..
                }
        )
    }

    /// Returns `true` if the tag is only applicable on `x86_64` platforms.
    pub fn is_x86_64(&self) -> bool {
        matches!(
            self,
            Self::Manylinux {
                arch: Arch::X86_64,
                ..
            } | Self::Manylinux1 {
                arch: Arch::X86_64,
                ..
            } | Self::Manylinux2010 {
                arch: Arch::X86_64,
                ..
            } | Self::Manylinux2014 {
                arch: Arch::X86_64,
                ..
            } | Self::Linux {
                arch: Arch::X86_64,
                ..
            } | Self::Musllinux {
                arch: Arch::X86_64,
                ..
            } | Self::Macos {
                binary_format: BinaryFormat::X86_64,
                ..
            } | Self::WinAmd64
        )
    }

    /// Returns `true` if the tag is only applicable on x86 platforms.
    pub fn is_x86(&self) -> bool {
        matches!(
            self,
            Self::Manylinux {
                arch: Arch::X86,
                ..
            } | Self::Manylinux1 {
                arch: Arch::X86,
                ..
            } | Self::Manylinux2010 {
                arch: Arch::X86,
                ..
            } | Self::Manylinux2014 {
                arch: Arch::X86,
                ..
            } | Self::Linux {
                arch: Arch::X86,
                ..
            } | Self::Musllinux {
                arch: Arch::X86,
                ..
            } | Self::Macos {
                binary_format: BinaryFormat::I386,
                ..
            } | Self::Win32
        )
    }
}

impl std::fmt::Display for PlatformTag {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Any => write!(f, "any"),
            Self::Manylinux { major, minor, arch } => {
                write!(f, "manylinux_{major}_{minor}_{arch}")
            }
            Self::Manylinux1 { arch } => write!(f, "manylinux1_{arch}"),
            Self::Manylinux2010 { arch } => write!(f, "manylinux2010_{arch}"),
            Self::Manylinux2014 { arch } => write!(f, "manylinux2014_{arch}"),
            Self::Linux { arch } => write!(f, "linux_{arch}"),
            Self::Musllinux { major, minor, arch } => {
                write!(f, "musllinux_{major}_{minor}_{arch}")
            }
            Self::Macos {
                major,
                minor,
                binary_format: format,
            } => write!(f, "macosx_{major}_{minor}_{format}"),
            Self::Win32 => write!(f, "win32"),
            Self::WinAmd64 => write!(f, "win_amd64"),
            Self::WinArm64 => write!(f, "win_arm64"),
            Self::WinIa64 => write!(f, "win_ia64"),
            Self::Android { api_level, arch } => write!(f, "android_{api_level}_{arch}"),
            Self::FreeBsd { release_arch } => write!(f, "freebsd_{release_arch}"),
            Self::NetBsd { release_arch } => write!(f, "netbsd_{release_arch}"),
            Self::OpenBsd { release_arch } => write!(f, "openbsd_{release_arch}"),
            Self::Dragonfly { release_arch } => write!(f, "dragonfly_{release_arch}"),
            Self::Haiku { release_arch } => write!(f, "haiku_{release_arch}"),
            Self::Illumos { release_arch } => write!(f, "illumos_{release_arch}"),
            Self::Solaris { release_arch } => write!(f, "solaris_{release_arch}_64bit"),
        }
    }
}

impl FromStr for PlatformTag {
    type Err = ParsePlatformTagError;

    /// Parse a [`PlatformTag`] from a string.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Match against any static variants.
        match s {
            "any" => return Ok(Self::Any),
            "win32" => return Ok(Self::Win32),
            "win_amd64" => return Ok(Self::WinAmd64),
            "win_arm64" => return Ok(Self::WinArm64),
            "win_ia64" => return Ok(Self::WinIa64),
            _ => {}
        }

        if let Some(rest) = s.strip_prefix("manylinux_") {
            // Ex) manylinux_2_17_x86_64
            let first_underscore = memchr::memchr(b'_', rest.as_bytes()).ok_or_else(|| {
                ParsePlatformTagError::InvalidFormat {
                    platform: "manylinux",
                    tag: s.to_string(),
                }
            })?;

            let second_underscore = memchr::memchr(b'_', &rest.as_bytes()[first_underscore + 1..])
                .map(|i| i + first_underscore + 1)
                .ok_or_else(|| ParsePlatformTagError::InvalidFormat {
                    platform: "manylinux",
                    tag: s.to_string(),
                })?;

            let major = rest[..first_underscore].parse().map_err(|_| {
                ParsePlatformTagError::InvalidMajorVersion {
                    platform: "manylinux",
                    tag: s.to_string(),
                }
            })?;

            let minor = rest[first_underscore + 1..second_underscore]
                .parse()
                .map_err(|_| ParsePlatformTagError::InvalidMinorVersion {
                    platform: "manylinux",
                    tag: s.to_string(),
                })?;

            let arch_str = &rest[second_underscore + 1..];
            if arch_str.is_empty() {
                return Err(ParsePlatformTagError::InvalidFormat {
                    platform: "manylinux",
                    tag: s.to_string(),
                });
            }

            let arch = arch_str
                .parse()
                .map_err(|_| ParsePlatformTagError::InvalidArch {
                    platform: "manylinux",
                    tag: s.to_string(),
                })?;

            return Ok(Self::Manylinux { major, minor, arch });
        }

        if let Some(rest) = s.strip_prefix("manylinux1_") {
            // Ex) manylinux1_x86_64
            let arch = rest
                .parse()
                .map_err(|_| ParsePlatformTagError::InvalidArch {
                    platform: "manylinux1",
                    tag: s.to_string(),
                })?;
            return Ok(Self::Manylinux1 { arch });
        }

        if let Some(rest) = s.strip_prefix("manylinux2010_") {
            // Ex) manylinux2010_x86_64
            let arch = rest
                .parse()
                .map_err(|_| ParsePlatformTagError::InvalidArch {
                    platform: "manylinux2010",
                    tag: s.to_string(),
                })?;
            return Ok(Self::Manylinux2010 { arch });
        }

        if let Some(rest) = s.strip_prefix("manylinux2014_") {
            // Ex) manylinux2014_x86_64
            let arch = rest
                .parse()
                .map_err(|_| ParsePlatformTagError::InvalidArch {
                    platform: "manylinux2014",
                    tag: s.to_string(),
                })?;
            return Ok(Self::Manylinux2014 { arch });
        }

        if let Some(rest) = s.strip_prefix("linux_") {
            // Ex) linux_x86_64
            let arch = rest
                .parse()
                .map_err(|_| ParsePlatformTagError::InvalidArch {
                    platform: "linux",
                    tag: s.to_string(),
                })?;
            return Ok(Self::Linux { arch });
        }

        if let Some(rest) = s.strip_prefix("musllinux_") {
            // Ex) musllinux_1_1_x86_64
            let first_underscore = memchr::memchr(b'_', rest.as_bytes()).ok_or_else(|| {
                ParsePlatformTagError::InvalidFormat {
                    platform: "musllinux",
                    tag: s.to_string(),
                }
            })?;

            let second_underscore = memchr::memchr(b'_', &rest.as_bytes()[first_underscore + 1..])
                .map(|i| i + first_underscore + 1)
                .ok_or_else(|| ParsePlatformTagError::InvalidFormat {
                    platform: "musllinux",
                    tag: s.to_string(),
                })?;

            let major = rest[..first_underscore].parse().map_err(|_| {
                ParsePlatformTagError::InvalidMajorVersion {
                    platform: "musllinux",
                    tag: s.to_string(),
                }
            })?;

            let minor = rest[first_underscore + 1..second_underscore]
                .parse()
                .map_err(|_| ParsePlatformTagError::InvalidMinorVersion {
                    platform: "musllinux",
                    tag: s.to_string(),
                })?;

            let arch_str = &rest[second_underscore + 1..];
            if arch_str.is_empty() {
                return Err(ParsePlatformTagError::InvalidFormat {
                    platform: "musllinux",
                    tag: s.to_string(),
                });
            }

            let arch = arch_str
                .parse()
                .map_err(|_| ParsePlatformTagError::InvalidArch {
                    platform: "musllinux",
                    tag: s.to_string(),
                })?;

            return Ok(Self::Musllinux { major, minor, arch });
        }

        if let Some(rest) = s.strip_prefix("macosx_") {
            // Ex) macosx_11_0_arm64
            let first_underscore = memchr::memchr(b'_', rest.as_bytes()).ok_or_else(|| {
                ParsePlatformTagError::InvalidFormat {
                    platform: "macosx",
                    tag: s.to_string(),
                }
            })?;

            let second_underscore = memchr::memchr(b'_', &rest.as_bytes()[first_underscore + 1..])
                .map(|i| i + first_underscore + 1)
                .ok_or_else(|| ParsePlatformTagError::InvalidFormat {
                    platform: "macosx",
                    tag: s.to_string(),
                })?;

            let major = rest[..first_underscore].parse().map_err(|_| {
                ParsePlatformTagError::InvalidMajorVersion {
                    platform: "macosx",
                    tag: s.to_string(),
                }
            })?;

            let minor = rest[first_underscore + 1..second_underscore]
                .parse()
                .map_err(|_| ParsePlatformTagError::InvalidMinorVersion {
                    platform: "macosx",
                    tag: s.to_string(),
                })?;

            let binary_format_str = &rest[second_underscore + 1..];
            if binary_format_str.is_empty() {
                return Err(ParsePlatformTagError::InvalidFormat {
                    platform: "macosx",
                    tag: s.to_string(),
                });
            }

            let binary_format =
                binary_format_str
                    .parse()
                    .map_err(|_| ParsePlatformTagError::InvalidArch {
                        platform: "macosx",
                        tag: s.to_string(),
                    })?;

            return Ok(Self::Macos {
                major,
                minor,
                binary_format,
            });
        }

        if let Some(rest) = s.strip_prefix("android_") {
            // Ex) android_21_arm64
            let underscore = memchr::memchr(b'_', rest.as_bytes()).ok_or_else(|| {
                ParsePlatformTagError::InvalidFormat {
                    platform: "android",
                    tag: s.to_string(),
                }
            })?;

            let api_level =
                rest[..underscore]
                    .parse()
                    .map_err(|_| ParsePlatformTagError::InvalidApiLevel {
                        platform: "android",
                        tag: s.to_string(),
                    })?;

            let arch_str = &rest[underscore + 1..];
            if arch_str.is_empty() {
                return Err(ParsePlatformTagError::InvalidFormat {
                    platform: "android",
                    tag: s.to_string(),
                });
            }

            let arch = arch_str
                .parse()
                .map_err(|_| ParsePlatformTagError::InvalidArch {
                    platform: "android",
                    tag: s.to_string(),
                })?;

            return Ok(Self::Android { api_level, arch });
        }

        if let Some(rest) = s.strip_prefix("freebsd_") {
            // Ex) freebsd_13_x86_64 or freebsd_13_14_x86_64
            if rest.is_empty() {
                return Err(ParsePlatformTagError::InvalidFormat {
                    platform: "freebsd",
                    tag: s.to_string(),
                });
            }

            return Ok(Self::FreeBsd {
                release_arch: SmallString::from(rest),
            });
        }

        if let Some(rest) = s.strip_prefix("netbsd_") {
            // Ex) netbsd_9_x86_64
            if rest.is_empty() {
                return Err(ParsePlatformTagError::InvalidFormat {
                    platform: "netbsd",
                    tag: s.to_string(),
                });
            }

            return Ok(Self::NetBsd {
                release_arch: SmallString::from(rest),
            });
        }

        if let Some(rest) = s.strip_prefix("openbsd_") {
            // Ex) openbsd_7_x86_64
            if rest.is_empty() {
                return Err(ParsePlatformTagError::InvalidFormat {
                    platform: "openbsd",
                    tag: s.to_string(),
                });
            }

            return Ok(Self::OpenBsd {
                release_arch: SmallString::from(rest),
            });
        }

        if let Some(rest) = s.strip_prefix("dragonfly_") {
            // Ex) dragonfly_6_x86_64
            if rest.is_empty() {
                return Err(ParsePlatformTagError::InvalidFormat {
                    platform: "dragonfly",
                    tag: s.to_string(),
                });
            }

            return Ok(Self::Dragonfly {
                release_arch: SmallString::from(rest),
            });
        }

        if let Some(rest) = s.strip_prefix("haiku_") {
            // Ex) haiku_1_x86_64
            if rest.is_empty() {
                return Err(ParsePlatformTagError::InvalidFormat {
                    platform: "haiku",
                    tag: s.to_string(),
                });
            }

            return Ok(Self::Haiku {
                release_arch: SmallString::from(rest),
            });
        }

        if let Some(rest) = s.strip_prefix("illumos_") {
            // Ex) illumos_5_11_x86_64
            if rest.is_empty() {
                return Err(ParsePlatformTagError::InvalidFormat {
                    platform: "illumos",
                    tag: s.to_string(),
                });
            }

            return Ok(Self::Illumos {
                release_arch: SmallString::from(rest),
            });
        }

        if let Some(rest) = s.strip_prefix("solaris_") {
            // Ex) solaris_11_4_x86_64_64bit
            if rest.is_empty() {
                return Err(ParsePlatformTagError::InvalidFormat {
                    platform: "solaris",
                    tag: s.to_string(),
                });
            }

            if let Some(release_arch) = rest.strip_suffix("_64bit") {
                if !release_arch.is_empty() {
                    return Ok(Self::Solaris {
                        release_arch: SmallString::from(release_arch),
                    });
                }
            }

            return Err(ParsePlatformTagError::InvalidArch {
                platform: "solaris",
                tag: s.to_string(),
            });
        }

        Err(ParsePlatformTagError::UnknownFormat(s.to_string()))
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ParsePlatformTagError {
    #[error("Unknown platform tag format: {0}")]
    UnknownFormat(String),
    #[error("Invalid format for {platform} platform tag: {tag}")]
    InvalidFormat { platform: &'static str, tag: String },
    #[error("Invalid major version in {platform} platform tag: {tag}")]
    InvalidMajorVersion { platform: &'static str, tag: String },
    #[error("Invalid minor version in {platform} platform tag: {tag}")]
    InvalidMinorVersion { platform: &'static str, tag: String },
    #[error("Invalid architecture in {platform} platform tag: {tag}")]
    InvalidArch { platform: &'static str, tag: String },
    #[error("Invalid API level in {platform} platform tag: {tag}")]
    InvalidApiLevel { platform: &'static str, tag: String },
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use crate::platform_tag::{ParsePlatformTagError, PlatformTag};
    use crate::{Arch, BinaryFormat};

    #[test]
    fn any_platform() {
        assert_eq!(PlatformTag::from_str("any"), Ok(PlatformTag::Any));
        assert_eq!(PlatformTag::Any.to_string(), "any");
    }

    #[test]
    fn manylinux_platform() {
        let tag = PlatformTag::Manylinux {
            major: 2,
            minor: 24,
            arch: Arch::X86_64,
        };
        assert_eq!(
            PlatformTag::from_str("manylinux_2_24_x86_64").as_ref(),
            Ok(&tag)
        );
        assert_eq!(tag.to_string(), "manylinux_2_24_x86_64");

        assert_eq!(
            PlatformTag::from_str("manylinux_x_24_x86_64"),
            Err(ParsePlatformTagError::InvalidMajorVersion {
                platform: "manylinux",
                tag: "manylinux_x_24_x86_64".to_string()
            })
        );

        assert_eq!(
            PlatformTag::from_str("manylinux_2_x_x86_64"),
            Err(ParsePlatformTagError::InvalidMinorVersion {
                platform: "manylinux",
                tag: "manylinux_2_x_x86_64".to_string()
            })
        );

        assert_eq!(
            PlatformTag::from_str("manylinux_2_24_invalid"),
            Err(ParsePlatformTagError::InvalidArch {
                platform: "manylinux",
                tag: "manylinux_2_24_invalid".to_string()
            })
        );
    }

    #[test]
    fn manylinux1_platform() {
        let tag = PlatformTag::Manylinux1 { arch: Arch::X86_64 };
        assert_eq!(
            PlatformTag::from_str("manylinux1_x86_64").as_ref(),
            Ok(&tag)
        );
        assert_eq!(tag.to_string(), "manylinux1_x86_64");

        assert_eq!(
            PlatformTag::from_str("manylinux1_invalid"),
            Err(ParsePlatformTagError::InvalidArch {
                platform: "manylinux1",
                tag: "manylinux1_invalid".to_string()
            })
        );
    }

    #[test]
    fn manylinux2010_platform() {
        let tag = PlatformTag::Manylinux2010 { arch: Arch::X86_64 };
        assert_eq!(
            PlatformTag::from_str("manylinux2010_x86_64").as_ref(),
            Ok(&tag)
        );
        assert_eq!(tag.to_string(), "manylinux2010_x86_64");

        assert_eq!(
            PlatformTag::from_str("manylinux2010_invalid"),
            Err(ParsePlatformTagError::InvalidArch {
                platform: "manylinux2010",
                tag: "manylinux2010_invalid".to_string()
            })
        );
    }

    #[test]
    fn manylinux2014_platform() {
        let tag = PlatformTag::Manylinux2014 { arch: Arch::X86_64 };
        assert_eq!(
            PlatformTag::from_str("manylinux2014_x86_64").as_ref(),
            Ok(&tag)
        );
        assert_eq!(tag.to_string(), "manylinux2014_x86_64");

        assert_eq!(
            PlatformTag::from_str("manylinux2014_invalid"),
            Err(ParsePlatformTagError::InvalidArch {
                platform: "manylinux2014",
                tag: "manylinux2014_invalid".to_string()
            })
        );
    }

    #[test]
    fn linux_platform() {
        let tag = PlatformTag::Linux { arch: Arch::X86_64 };
        assert_eq!(PlatformTag::from_str("linux_x86_64").as_ref(), Ok(&tag));
        assert_eq!(tag.to_string(), "linux_x86_64");

        assert_eq!(
            PlatformTag::from_str("linux_invalid"),
            Err(ParsePlatformTagError::InvalidArch {
                platform: "linux",
                tag: "linux_invalid".to_string()
            })
        );
    }

    #[test]
    fn musllinux_platform() {
        let tag = PlatformTag::Musllinux {
            major: 1,
            minor: 2,
            arch: Arch::X86_64,
        };
        assert_eq!(
            PlatformTag::from_str("musllinux_1_2_x86_64").as_ref(),
            Ok(&tag)
        );
        assert_eq!(tag.to_string(), "musllinux_1_2_x86_64");

        assert_eq!(
            PlatformTag::from_str("musllinux_x_2_x86_64"),
            Err(ParsePlatformTagError::InvalidMajorVersion {
                platform: "musllinux",
                tag: "musllinux_x_2_x86_64".to_string()
            })
        );

        assert_eq!(
            PlatformTag::from_str("musllinux_1_x_x86_64"),
            Err(ParsePlatformTagError::InvalidMinorVersion {
                platform: "musllinux",
                tag: "musllinux_1_x_x86_64".to_string()
            })
        );

        assert_eq!(
            PlatformTag::from_str("musllinux_1_2_invalid"),
            Err(ParsePlatformTagError::InvalidArch {
                platform: "musllinux",
                tag: "musllinux_1_2_invalid".to_string()
            })
        );
    }

    #[test]
    fn macos_platform() {
        let tag = PlatformTag::Macos {
            major: 11,
            minor: 0,
            binary_format: BinaryFormat::Universal2,
        };
        assert_eq!(
            PlatformTag::from_str("macosx_11_0_universal2").as_ref(),
            Ok(&tag)
        );
        assert_eq!(tag.to_string(), "macosx_11_0_universal2");

        assert_eq!(
            PlatformTag::from_str("macosx_x_0_universal2"),
            Err(ParsePlatformTagError::InvalidMajorVersion {
                platform: "macosx",
                tag: "macosx_x_0_universal2".to_string()
            })
        );

        assert_eq!(
            PlatformTag::from_str("macosx_11_x_universal2"),
            Err(ParsePlatformTagError::InvalidMinorVersion {
                platform: "macosx",
                tag: "macosx_11_x_universal2".to_string()
            })
        );

        assert_eq!(
            PlatformTag::from_str("macosx_11_0_invalid"),
            Err(ParsePlatformTagError::InvalidArch {
                platform: "macosx",
                tag: "macosx_11_0_invalid".to_string()
            })
        );
    }

    #[test]
    fn win32_platform() {
        assert_eq!(PlatformTag::from_str("win32"), Ok(PlatformTag::Win32));
        assert_eq!(PlatformTag::Win32.to_string(), "win32");
    }

    #[test]
    fn win_amd64_platform() {
        assert_eq!(
            PlatformTag::from_str("win_amd64"),
            Ok(PlatformTag::WinAmd64)
        );
        assert_eq!(PlatformTag::WinAmd64.to_string(), "win_amd64");
    }

    #[test]
    fn win_arm64_platform() {
        assert_eq!(
            PlatformTag::from_str("win_arm64"),
            Ok(PlatformTag::WinArm64)
        );
        assert_eq!(PlatformTag::WinArm64.to_string(), "win_arm64");
    }

    #[test]
    fn freebsd_platform() {
        let tag = PlatformTag::FreeBsd {
            release_arch: "13_14_x86_64".into(),
        };
        assert_eq!(
            PlatformTag::from_str("freebsd_13_14_x86_64").as_ref(),
            Ok(&tag)
        );
        assert_eq!(tag.to_string(), "freebsd_13_14_x86_64");
    }

    #[test]
    fn illumos_platform() {
        let tag = PlatformTag::Illumos {
            release_arch: "5_11_x86_64".into(),
        };
        assert_eq!(
            PlatformTag::from_str("illumos_5_11_x86_64").as_ref(),
            Ok(&tag)
        );
        assert_eq!(tag.to_string(), "illumos_5_11_x86_64");
    }

    #[test]
    fn solaris_platform() {
        let tag = PlatformTag::Solaris {
            release_arch: "11_4_x86_64".into(),
        };
        assert_eq!(
            PlatformTag::from_str("solaris_11_4_x86_64_64bit").as_ref(),
            Ok(&tag)
        );
        assert_eq!(tag.to_string(), "solaris_11_4_x86_64_64bit");

        assert_eq!(
            PlatformTag::from_str("solaris_11_4_x86_64"),
            Err(ParsePlatformTagError::InvalidArch {
                platform: "solaris",
                tag: "solaris_11_4_x86_64".to_string()
            })
        );
    }

    #[test]
    fn unknown_platform() {
        assert_eq!(
            PlatformTag::from_str("unknown"),
            Err(ParsePlatformTagError::UnknownFormat("unknown".to_string()))
        );
        assert_eq!(
            PlatformTag::from_str(""),
            Err(ParsePlatformTagError::UnknownFormat(String::new()))
        );
    }
}
