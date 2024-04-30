use std::{
    fmt::{self, Display},
    str::FromStr,
};
use thiserror::Error;

#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub enum ImplementationName {
    Cpython,
}

#[derive(Debug, PartialEq, Clone)]
pub struct Platform {
    os: Os,
    arch: Arch,
    libc: Libc,
}

/// All supported operating systems.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Os {
    Windows,
    Linux,
    Macos,
    FreeBsd,
    NetBsd,
    OpenBsd,
    Dragonfly,
    Illumos,
    Haiku,
}

/// All supported CPU architectures
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Arch {
    Aarch64,
    Armv6L,
    Armv7L,
    Powerpc64Le,
    Powerpc64,
    X86,
    X86_64,
    S390X,
}

#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub enum Libc {
    Gnu,
    Musl,
    None,
}

#[derive(Error, Debug)]
pub enum PythonSelectorError {
    #[error("Operating system not supported: {0}")]
    OsNotSupported(String),
    #[error("Architecture not supported: {0}")]
    ArchNotSupported(String),
    #[error("Libc type could not be detected")]
    LibcNotDetected(),
    #[error("Implementation not supported: {0}")]
    ImplementationNotSupported(String),
}

impl ImplementationName {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Cpython => "cpython",
        }
    }
}

impl FromStr for ImplementationName {
    type Err = PythonSelectorError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "cpython" => Ok(Self::Cpython),
            _ => Err(PythonSelectorError::ImplementationNotSupported(
                s.to_string(),
            )),
        }
    }
}

impl Display for ImplementationName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Platform {
    pub fn new(os: Os, arch: Arch, libc: Libc) -> Self {
        Self { os, arch, libc }
    }
    pub fn from_env() -> Result<Self, PythonSelectorError> {
        Ok(Self::new(
            Os::from_env()?,
            Arch::from_env()?,
            Libc::from_env()?,
        ))
    }
}

impl fmt::Display for Os {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Self::Windows => write!(f, "Windows"),
            Self::Macos => write!(f, "MacOS"),
            Self::FreeBsd => write!(f, "FreeBSD"),
            Self::NetBsd => write!(f, "NetBSD"),
            Self::Linux => write!(f, "Linux"),
            Self::OpenBsd => write!(f, "OpenBSD"),
            Self::Dragonfly => write!(f, "DragonFly"),
            Self::Illumos => write!(f, "Illumos"),
            Self::Haiku => write!(f, "Haiku"),
        }
    }
}

impl Os {
    pub(crate) fn from_env() -> Result<Self, PythonSelectorError> {
        Self::from_str(std::env::consts::OS)
    }
}

impl FromStr for Os {
    type Err = PythonSelectorError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "windows" => Ok(Self::Windows),
            "linux" => Ok(Self::Linux),
            "macos" => Ok(Self::Macos),
            "freebsd" => Ok(Self::FreeBsd),
            "netbsd" => Ok(Self::NetBsd),
            "openbsd" => Ok(Self::OpenBsd),
            "dragonfly" => Ok(Self::Dragonfly),
            "illumos" => Ok(Self::Illumos),
            "haiku" => Ok(Self::Haiku),
            _ => Err(PythonSelectorError::OsNotSupported(s.to_string())),
        }
    }
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

impl FromStr for Arch {
    type Err = PythonSelectorError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "aarch64" | "arm64" => Ok(Self::Aarch64),
            "armv6l" => Ok(Self::Armv6L),
            "armv7l" => Ok(Self::Armv7L),
            "powerpc64le" | "ppc64le" => Ok(Self::Powerpc64Le),
            "powerpc64" | "ppc64" => Ok(Self::Powerpc64),
            "x86" | "i686" | "i386" => Ok(Self::X86),
            "x86_64" | "amd64" => Ok(Self::X86_64),
            "s390x" => Ok(Self::S390X),
            _ => Err(PythonSelectorError::ArchNotSupported(s.to_string())),
        }
    }
}

impl Arch {
    pub(crate) fn from_env() -> Result<Self, PythonSelectorError> {
        Self::from_str(std::env::consts::ARCH)
    }
}

impl Libc {
    pub(crate) fn from_env() -> Result<Self, PythonSelectorError> {
        // TODO(zanieb): Perform this lookup
        match std::env::consts::OS {
            "linux" => Ok(Libc::Gnu),
            "windows" | "macos" => Ok(Libc::None),
            _ => Err(PythonSelectorError::LibcNotDetected()),
        }
    }
}

impl fmt::Display for Libc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Libc::Gnu => f.write_str("gnu"),
            Libc::None => f.write_str("none"),
            Libc::Musl => f.write_str("musl"),
        }
    }
}
