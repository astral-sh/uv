use std::fmt::Display;
use std::ops::Deref;
use std::{fmt, str::FromStr};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Unknown operating system: {0}")]
    UnknownOs(String),
    #[error("Unknown architecture: {0}")]
    UnknownArch(String),
    #[error("Unknown libc environment: {0}")]
    UnknownLibc(String),
}

#[derive(Debug, Eq, PartialEq, Clone, Copy, Hash)]
pub struct Arch(pub(crate) target_lexicon::Architecture);

#[derive(Debug, Eq, PartialEq, Clone, Copy, Hash)]
pub struct Os(pub(crate) target_lexicon::OperatingSystem);

#[derive(Debug, Eq, PartialEq, Clone, Copy, Hash)]
pub enum Libc {
    Some(target_lexicon::Environment),
    None,
}

impl Libc {
    pub(crate) fn from_env() -> Self {
        match std::env::consts::OS {
            // TODO(zanieb): On Linux, we use the uv target host to determine the libc variant
            // but we should only use this as a fallback and should instead inspect the
            // machine's `/bin/sh` (or similar).
            "linux" => Self::Some(target_lexicon::Environment::Gnu),
            "windows" | "macos" => Self::None,
            // Use `None` on platforms without explicit support.
            _ => Self::None,
        }
    }
}

impl FromStr for Libc {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "gnu" => Ok(Self::Some(target_lexicon::Environment::Gnu)),
            "musl" => Ok(Self::Some(target_lexicon::Environment::Musl)),
            "none" => Ok(Self::None),
            _ => Err(Error::UnknownLibc(s.to_string())),
        }
    }
}

impl Os {
    pub fn from_env() -> Self {
        Self(target_lexicon::HOST.operating_system)
    }
}

impl Arch {
    pub fn from_env() -> Self {
        Self(target_lexicon::HOST.architecture)
    }
}

impl Display for Libc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Some(env) => write!(f, "{env}"),
            Self::None => write!(f, "none"),
        }
    }
}

impl Display for Os {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &**self {
            target_lexicon::OperatingSystem::Darwin => write!(f, "macos"),
            inner => write!(f, "{inner}"),
        }
    }
}

impl Display for Arch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &**self {
            target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686) => {
                write!(f, "x86")
            }
            inner => write!(f, "{inner}"),
        }
    }
}

impl FromStr for Os {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let inner = match s {
            "macos" => target_lexicon::OperatingSystem::Darwin,
            _ => target_lexicon::OperatingSystem::from_str(s)
                .map_err(|()| Error::UnknownOs(s.to_string()))?,
        };
        if matches!(inner, target_lexicon::OperatingSystem::Unknown) {
            return Err(Error::UnknownOs(s.to_string()));
        }
        Ok(Self(inner))
    }
}

impl FromStr for Arch {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let inner = match s {
            // Allow users to specify "x86" as a shorthand for the "i686" variant, they should not need
            // to specify the exact architecture and this variant is what we have downloads for.
            "x86" => target_lexicon::Architecture::X86_32(target_lexicon::X86_32Architecture::I686),
            _ => target_lexicon::Architecture::from_str(s)
                .map_err(|()| Error::UnknownArch(s.to_string()))?,
        };
        if matches!(inner, target_lexicon::Architecture::Unknown) {
            return Err(Error::UnknownArch(s.to_string()));
        }
        Ok(Self(inner))
    }
}

impl Deref for Arch {
    type Target = target_lexicon::Architecture;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Deref for Os {
    type Target = target_lexicon::OperatingSystem;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<&platform_tags::Arch> for Arch {
    fn from(value: &platform_tags::Arch) -> Self {
        match value {
            platform_tags::Arch::Aarch64 => Self(target_lexicon::Architecture::Aarch64(
                target_lexicon::Aarch64Architecture::Aarch64,
            )),
            platform_tags::Arch::Armv6L => Self(target_lexicon::Architecture::Arm(
                target_lexicon::ArmArchitecture::Armv6,
            )),
            platform_tags::Arch::Armv7L => Self(target_lexicon::Architecture::Arm(
                target_lexicon::ArmArchitecture::Armv7,
            )),
            platform_tags::Arch::S390X => Self(target_lexicon::Architecture::S390x),
            platform_tags::Arch::Powerpc64 => Self(target_lexicon::Architecture::Powerpc64),
            platform_tags::Arch::Powerpc64Le => Self(target_lexicon::Architecture::Powerpc64le),
            platform_tags::Arch::X86 => Self(target_lexicon::Architecture::X86_32(
                target_lexicon::X86_32Architecture::I686,
            )),
            platform_tags::Arch::X86_64 => Self(target_lexicon::Architecture::X86_64),
        }
    }
}

impl From<&platform_tags::Os> for Libc {
    fn from(value: &platform_tags::Os) -> Self {
        match value {
            platform_tags::Os::Manylinux { .. } => Self::Some(target_lexicon::Environment::Gnu),
            platform_tags::Os::Musllinux { .. } => Self::Some(target_lexicon::Environment::Musl),
            _ => Self::None,
        }
    }
}

impl From<&platform_tags::Os> for Os {
    fn from(value: &platform_tags::Os) -> Self {
        match value {
            platform_tags::Os::Dragonfly { .. } => Self(target_lexicon::OperatingSystem::Dragonfly),
            platform_tags::Os::FreeBsd { .. } => Self(target_lexicon::OperatingSystem::Freebsd),
            platform_tags::Os::Haiku { .. } => Self(target_lexicon::OperatingSystem::Haiku),
            platform_tags::Os::Illumos { .. } => Self(target_lexicon::OperatingSystem::Illumos),
            platform_tags::Os::Macos { .. } => Self(target_lexicon::OperatingSystem::Darwin),
            platform_tags::Os::Manylinux { .. } | platform_tags::Os::Musllinux { .. } => {
                Self(target_lexicon::OperatingSystem::Linux)
            }
            platform_tags::Os::NetBsd { .. } => Self(target_lexicon::OperatingSystem::Netbsd),
            platform_tags::Os::OpenBsd { .. } => Self(target_lexicon::OperatingSystem::Openbsd),
            platform_tags::Os::Windows => Self(target_lexicon::OperatingSystem::Windows),
        }
    }
}
