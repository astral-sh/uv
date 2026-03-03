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
    #[error("Invalid Android architecture: {0}")]
    InvalidAndroidArch(Arch),
    #[error("Invalid iOS simulator architecture: {0}")]
    InvalidIosSimulatorArch(Arch),
    #[error("Invalid iOS device architecture: {0}")]
    InvalidIosDeviceArch(Arch),
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
    Manylinux {
        major: u16,
        minor: u16,
    },
    Musllinux {
        major: u16,
        minor: u16,
    },
    Windows,
    Pyodide {
        major: u16,
        minor: u16,
    },
    Macos {
        major: u16,
        minor: u16,
    },
    FreeBsd {
        release: String,
    },
    NetBsd {
        release: String,
    },
    OpenBsd {
        release: String,
    },
    Dragonfly {
        release: String,
    },
    Illumos {
        release: String,
        arch: String,
    },
    Haiku {
        release: String,
    },
    Android {
        api_level: u16,
    },
    Ios {
        major: u16,
        minor: u16,
        simulator: bool,
    },
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
            Self::Ios { .. } => write!(f, "ios"),
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
        write!(f, "{}", self.name())
    }
}

impl FromStr for Arch {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "aarch64" => Ok(Self::Aarch64),
            "armv5tel" => Ok(Self::Armv5TEL),
            "armv6l" => Ok(Self::Armv6L),
            // armv8l is 32-bit ARM running on ARMv8 hardware, compatible with armv7l
            "armv7l" | "armv8l" => Ok(Self::Armv7L),
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

    /// Returns the standard name of the architecture.
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

    /// Represents the hardware platform.
    ///
    /// This is the same as the native platform's `uname -m` output.
    ///
    /// Based on: <https://github.com/PyO3/maturin/blob/8ab42219247277fee513eac753a3e90e76cd46b9/src/target/mod.rs#L131>
    pub fn machine(&self) -> &'static str {
        match self {
            Self::Aarch64 => "arm64",
            Self::Armv5TEL | Self::Armv6L | Self::Armv7L => "arm",
            Self::Powerpc | Self::Powerpc64Le | Self::Powerpc64 => "powerpc",
            Self::X86 => "i386",
            Self::X86_64 => "amd64",
            Self::Riscv64 => "riscv",
            Self::Wasm32 => "wasm32",
            Self::S390X => "s390x",
            Self::LoongArch64 => "loongarch64",
        }
    }
}
