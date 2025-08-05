use crate::Error;
use std::fmt;
use std::fmt::Display;
use std::ops::Deref;
use std::str::FromStr;

#[derive(Debug, Eq, PartialEq, Clone, Copy, Hash)]
pub struct Os(pub(crate) target_lexicon::OperatingSystem);

impl Os {
    pub fn new(os: target_lexicon::OperatingSystem) -> Self {
        Self(os)
    }

    pub fn from_env() -> Self {
        Self(target_lexicon::HOST.operating_system)
    }

    pub fn is_windows(&self) -> bool {
        matches!(self.0, target_lexicon::OperatingSystem::Windows)
    }
}

impl Display for Os {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &**self {
            target_lexicon::OperatingSystem::Darwin(_) => write!(f, "macos"),
            inner => write!(f, "{inner}"),
        }
    }
}

impl FromStr for Os {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let inner = match s {
            "macos" => target_lexicon::OperatingSystem::Darwin(None),
            _ => target_lexicon::OperatingSystem::from_str(s)
                .map_err(|()| Error::UnknownOs(s.to_string()))?,
        };
        if matches!(inner, target_lexicon::OperatingSystem::Unknown) {
            return Err(Error::UnknownOs(s.to_string()));
        }
        Ok(Self(inner))
    }
}

impl Deref for Os {
    type Target = target_lexicon::OperatingSystem;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<&uv_platform_tags::Os> for Os {
    fn from(value: &uv_platform_tags::Os) -> Self {
        match value {
            uv_platform_tags::Os::Dragonfly { .. } => {
                Self::new(target_lexicon::OperatingSystem::Dragonfly)
            }
            uv_platform_tags::Os::FreeBsd { .. } => {
                Self::new(target_lexicon::OperatingSystem::Freebsd)
            }
            uv_platform_tags::Os::Haiku { .. } => Self::new(target_lexicon::OperatingSystem::Haiku),
            uv_platform_tags::Os::Illumos { .. } => {
                Self::new(target_lexicon::OperatingSystem::Illumos)
            }
            uv_platform_tags::Os::Macos { .. } => {
                Self::new(target_lexicon::OperatingSystem::Darwin(None))
            }
            uv_platform_tags::Os::Manylinux { .. }
            | uv_platform_tags::Os::Musllinux { .. }
            | uv_platform_tags::Os::Android { .. } => {
                Self::new(target_lexicon::OperatingSystem::Linux)
            }
            uv_platform_tags::Os::NetBsd { .. } => {
                Self::new(target_lexicon::OperatingSystem::Netbsd)
            }
            uv_platform_tags::Os::OpenBsd { .. } => {
                Self::new(target_lexicon::OperatingSystem::Openbsd)
            }
            uv_platform_tags::Os::Windows => Self::new(target_lexicon::OperatingSystem::Windows),
            uv_platform_tags::Os::Pyodide { .. } => {
                Self::new(target_lexicon::OperatingSystem::Emscripten)
            }
        }
    }
}
