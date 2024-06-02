//! Abstractions for understanding the current platform (operating system and architecture).

use std::{fmt, io};

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum PlatformError {
    #[error(transparent)]
    IOError(#[from] io::Error),
    #[error("Failed to detect the operating system version: {0}")]
    OsVersionDetectionError(String),
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
pub struct Platform {
    os: Os,
    arch: Arch,
}

impl Platform {
    /// Create a new platform from the given operating system and architecture.
    pub const fn new(os: Os, arch: Arch) -> Self {
        Self { os, arch }
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
#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(tag = "name", rename_all = "lowercase")]
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
#[derive(Debug, Clone, Copy, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Arch {
    #[serde(alias = "arm64")]
    Aarch64,
    Armv6L,
    Armv7L,
    #[serde(alias = "ppc64le")]
    Powerpc64Le,
    #[serde(alias = "ppc64")]
    Powerpc64,
    #[serde(alias = "i386", alias = "i686")]
    X86,
    #[serde(alias = "amd64")]
    X86_64,
    S390X,
}

impl fmt::Display for Arch {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Self::Aarch64 => write!(f, "aarch64"),
            Self::Armv6L => write!(f, "armv6l"),
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
    /// Returns the oldest possible `manylinux` tag for this architecture, if it supports
    /// `manylinux`.
    pub fn get_minimum_manylinux_minor(&self) -> Option<u16> {
        match self {
            // manylinux 2014
            Self::Aarch64 | Self::Armv7L | Self::Powerpc64 | Self::Powerpc64Le | Self::S390X => {
                Some(17)
            }
            // manylinux 1
            Self::X86 | Self::X86_64 => Some(5),
            // unsupported
            Self::Armv6L => None,
        }
    }
}
