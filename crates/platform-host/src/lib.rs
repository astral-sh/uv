//! Abstractions for understanding the current platform (operating system and architecture).

use std::{fmt, io};

use platform_info::{PlatformInfo, PlatformInfoAPI, UNameAPI};
use thiserror::Error;

use crate::linux::detect_linux_libc;
use crate::mac_os::get_mac_os_version;

mod linux;
mod mac_os;

#[derive(Error, Debug)]
pub enum PlatformError {
    #[error(transparent)]
    IOError(#[from] io::Error),
    #[error("Failed to detect the operating system version: {0}")]
    OsVersionDetectionError(String),
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Platform {
    os: Os,
    arch: Arch,
}

impl Platform {
    /// Create a new platform from the given operating system and architecture.
    pub fn new(os: Os, arch: Arch) -> Self {
        Self { os, arch }
    }

    /// Create a new platform from the current operating system and architecture.
    pub fn current() -> Result<Self, PlatformError> {
        let os = Os::current()?;
        let arch = Arch::current()?;
        Ok(Self { os, arch })
    }

    /// Return the platform's operating system.
    pub fn os(&self) -> &Os {
        &self.os
    }

    /// Return the platform's architecture.
    pub fn arch(&self) -> Arch {
        self.arch
    }
}

/// All supported operating systems.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Os {
    Manylinux { major: u16, minor: u16 },
    Musllinux { major: u16, minor: u16 },
    Windows,
    Macos { major: u16, minor: u16 },
    FreeBsd { release: String },
    NetBsd { release: String },
    OpenBsd { release: String },
    Dragonfly { release: String },
    Illumos { release: String, arch: String },
    Haiku { release: String },
}

impl Os {
    pub fn current() -> Result<Self, PlatformError> {
        let target_triple = target_lexicon::HOST;

        let os = match target_triple.operating_system {
            target_lexicon::OperatingSystem::Linux => detect_linux_libc()?,
            target_lexicon::OperatingSystem::Windows => Self::Windows,
            target_lexicon::OperatingSystem::MacOSX { major, minor, .. } => {
                Self::Macos { major, minor }
            }
            target_lexicon::OperatingSystem::Darwin => {
                let (major, minor) = get_mac_os_version()?;
                Self::Macos { major, minor }
            }
            target_lexicon::OperatingSystem::Netbsd => Self::NetBsd {
                release: Self::platform_info()?
                    .release()
                    .to_string_lossy()
                    .to_string(),
            },
            target_lexicon::OperatingSystem::Freebsd => Self::FreeBsd {
                release: Self::platform_info()?
                    .release()
                    .to_string_lossy()
                    .to_string(),
            },
            target_lexicon::OperatingSystem::Openbsd => Self::OpenBsd {
                release: Self::platform_info()?
                    .release()
                    .to_string_lossy()
                    .to_string(),
            },
            target_lexicon::OperatingSystem::Dragonfly => Self::Dragonfly {
                release: Self::platform_info()?
                    .release()
                    .to_string_lossy()
                    .to_string(),
            },
            target_lexicon::OperatingSystem::Illumos => {
                let platform_info = Self::platform_info()?;
                Self::Illumos {
                    release: platform_info.release().to_string_lossy().to_string(),
                    arch: platform_info.machine().to_string_lossy().to_string(),
                }
            }
            target_lexicon::OperatingSystem::Haiku => Self::Haiku {
                release: Self::platform_info()?
                    .release()
                    .to_string_lossy()
                    .to_string(),
            },
            unsupported => {
                return Err(PlatformError::OsVersionDetectionError(format!(
                    "The operating system {unsupported:?} is not supported"
                )));
            }
        };
        Ok(os)
    }

    fn platform_info() -> Result<PlatformInfo, PlatformError> {
        PlatformInfo::new().map_err(|err| PlatformError::OsVersionDetectionError(err.to_string()))
    }
}

impl fmt::Display for Os {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Self::Manylinux { .. } => write!(f, "Manylinux"),
            Self::Musllinux { .. } => write!(f, "Musllinux"),
            Self::Windows => write!(f, "Windows"),
            Self::Macos { .. } => write!(f, "MacOS"),
            Self::FreeBsd { .. } => write!(f, "FreeBSD"),
            Self::NetBsd { .. } => write!(f, "NetBSD"),
            Self::OpenBsd { .. } => write!(f, "OpenBSD"),
            Self::Dragonfly { .. } => write!(f, "DragonFly"),
            Self::Illumos { .. } => write!(f, "Illumos"),
            Self::Haiku { .. } => write!(f, "Haiku"),
        }
    }
}

/// All supported CPU architectures
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Arch {
    Aarch64,
    Armv7L,
    Powerpc64Le,
    Powerpc64,
    X86,
    X86_64,
    S390X,
}

impl fmt::Display for Arch {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Self::Aarch64 => write!(f, "aarch64"),
            Self::Armv7L => write!(f, "armv7l"),
            Self::Powerpc64Le => write!(f, "ppc64le"),
            Self::Powerpc64 => write!(f, "ppc64"),
            Self::X86 => write!(f, "i686"),
            Self::X86_64 => write!(f, "x86_64"),
            Self::S390X => write!(f, "s390x"),
        }
    }
}

impl Arch {
    pub fn current() -> Result<Self, PlatformError> {
        let target_triple = target_lexicon::HOST;
        let arch = match target_triple.architecture {
            target_lexicon::Architecture::X86_64 => Self::X86_64,
            target_lexicon::Architecture::X86_32(_) => Self::X86,
            target_lexicon::Architecture::Arm(_) => Self::Armv7L,
            target_lexicon::Architecture::Aarch64(_) => Self::Aarch64,
            target_lexicon::Architecture::Powerpc64 => Self::Powerpc64,
            target_lexicon::Architecture::Powerpc64le => Self::Powerpc64Le,
            target_lexicon::Architecture::S390x => Self::S390X,
            unsupported => {
                return Err(PlatformError::OsVersionDetectionError(format!(
                    "The architecture {unsupported} is not supported"
                )));
            }
        };
        Ok(arch)
    }

    /// Returns the oldest possible Manylinux tag for this architecture
    pub fn get_minimum_manylinux_minor(&self) -> u16 {
        match self {
            // manylinux 2014
            Self::Aarch64 | Self::Armv7L | Self::Powerpc64 | Self::Powerpc64Le | Self::S390X => 17,
            // manylinux 1
            Self::X86 | Self::X86_64 => 5,
        }
    }
}
