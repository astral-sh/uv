//! Abstractions for understanding the current platform (operating system and architecture).

use std::str::FromStr;
use std::{fmt, io};

use thiserror::Error;

#[derive(Error, Debug)]
pub enum PlatformError {
    #[error(transparent)]
    IOError(#[from] io::Error),
    #[error("Failed to detect the operating system version: {0}")]
    OsVersionDetectionError(String),
}

#[derive(Debug, Clone, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
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
#[derive(Debug, Clone, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(tag = "name", rename_all = "lowercase")]
pub enum Os {
    Manylinux { major: u16, minor: u16 },
    Musllinux { major: u16, minor: u16 },
    Windows,
    Pyodide { major: u16, minor: u16 },
    Macos { major: u16, minor: u16 },
    FreeBsd { release: String },
    NetBsd { release: String },
    OpenBsd { release: String },
    Dragonfly { release: String },
    Illumos { release: String, arch: String },
    Haiku { release: String },
    Android { api_level: u16 },
}

impl fmt::Display for Os {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Manylinux { .. } => write!(f, "manylinux"),
            Self::Musllinux { .. } => write!(f, "musllinux"),
            Self::Windows => write!(f, "windows"),
            Self::Macos { .. } => write!(f, "macos"),
            Self::FreeBsd { .. } => write!(f, "freebsd"),
            Self::NetBsd { .. } => write!(f, "netbsd"),
            Self::OpenBsd { .. } => write!(f, "openbsd"),
            Self::Dragonfly { .. } => write!(f, "dragonfly"),
            Self::Illumos { .. } => write!(f, "illumos"),
            Self::Haiku { .. } => write!(f, "haiku"),
            Self::Android { .. } => write!(f, "android"),
            Self::Pyodide { .. } => write!(f, "pyodide"),
        }
    }
}

/// All supported CPU architectures
#[derive(
    Debug,
    Copy,
    Clone,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Hash,
    rkyv::Archive,
    rkyv::Deserialize,
    rkyv::Serialize,
    serde::Deserialize,
    serde::Serialize,
)]
#[rkyv(derive(Debug))]
#[serde(rename_all = "lowercase")]
pub enum Arch {
    #[serde(alias = "arm64")]
    Aarch64,
    Armv5TEL,
    Armv6L,
    #[serde(alias = "armv8l")]
    Armv7L,
    #[serde(alias = "ppc64le")]
    Powerpc64Le,
    #[serde(alias = "ppc64")]
    Powerpc64,
    #[serde(alias = "ppc")]
    Powerpc,
    #[serde(alias = "i386", alias = "i686")]
    X86,
    #[serde(alias = "amd64")]
    X86_64,
    S390X,
    LoongArch64,
    Riscv64,
    Wasm32,
}

impl fmt::Display for Arch {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Self::Aarch64 => write!(f, "aarch64"),
            Self::Armv5TEL => write!(f, "armv5tel"),
            Self::Armv6L => write!(f, "armv6l"),
            Self::Armv7L => write!(f, "armv7l"),
            Self::Powerpc64Le => write!(f, "ppc64le"),
            Self::Powerpc64 => write!(f, "ppc64"),
            Self::Powerpc => write!(f, "ppc"),
            Self::X86 => write!(f, "i686"),
            Self::X86_64 => write!(f, "x86_64"),
            Self::S390X => write!(f, "s390x"),
            Self::LoongArch64 => write!(f, "loongarch64"),
            Self::Riscv64 => write!(f, "riscv64"),
            Self::Wasm32 => write!(f, "wasm32"),
        }
    }
}

impl FromStr for Arch {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "aarch64" => Ok(Self::Aarch64),
            "armv5tel" => Ok(Self::Armv5TEL),
            "armv6l" => Ok(Self::Armv6L),
            "armv7l" => Ok(Self::Armv7L),
            "ppc64le" => Ok(Self::Powerpc64Le),
            "ppc64" => Ok(Self::Powerpc64),
            "ppc" => Ok(Self::Powerpc),
            "i686" => Ok(Self::X86),
            "x86_64" => Ok(Self::X86_64),
            "s390x" => Ok(Self::S390X),
            "loongarch64" => Ok(Self::LoongArch64),
            "riscv64" => Ok(Self::Riscv64),
            _ => Err(format!("Unknown architecture: {s}")),
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
            // manylinux_2_31
            Self::Riscv64 => Some(31),
            // manylinux_2_36
            Self::LoongArch64 => Some(36),
            // unsupported
            Self::Powerpc | Self::Armv5TEL | Self::Armv6L | Self::Wasm32 => None,
        }
    }

    /// Returns the canonical name of the architecture.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Aarch64 => "aarch64",
            Self::Armv5TEL => "armv5tel",
            Self::Armv6L => "armv6l",
            Self::Armv7L => "armv7l",
            Self::Powerpc64Le => "ppc64le",
            Self::Powerpc64 => "ppc64",
            Self::Powerpc => "ppc",
            Self::X86 => "i686",
            Self::X86_64 => "x86_64",
            Self::S390X => "s390x",
            Self::LoongArch64 => "loongarch64",
            Self::Riscv64 => "riscv64",
            Self::Wasm32 => "wasm32",
        }
    }

    /// Returns an iterator over all supported architectures.
    pub fn iter() -> impl Iterator<Item = Self> {
        [
            Self::Aarch64,
            Self::Armv5TEL,
            Self::Armv6L,
            Self::Armv7L,
            Self::Powerpc64Le,
            Self::Powerpc64,
            Self::Powerpc,
            Self::X86,
            Self::X86_64,
            Self::S390X,
            Self::LoongArch64,
            Self::Riscv64,
        ]
        .iter()
        .copied()
    }
}
