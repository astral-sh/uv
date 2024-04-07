use thiserror::Error;

#[derive(Error, Debug)]
pub enum DownloadMetadataError {
    #[error("operating system not supported: {0}")]
    OsNotSupported(&'static str),
    #[error("architecture not supported: {0}")]
    ArchNotSupported(&'static str),
    #[error("libc type could not be detected")]
    LibcNotDetected(),
}

#[derive(Debug, PartialEq)]
pub struct PythonDownloadMetadata {
    name: ImplementationName,
    arch: Arch,
    os: Os,
    libc: Libc,
    major: i64,
    minor: i64,
    patch: i64,
    url: &'static str,
    sha256: Option<&'static str>,
}

#[derive(Debug, PartialEq)]
pub enum Arch {
    Arm64,
    I686,
    Ppc64Le,
    S390X,
    Windows,
    X86_64,
}

#[derive(Debug, PartialEq)]
pub enum Libc {
    Gnu,
    Musl,
    None,
}

#[derive(Debug, PartialEq)]
pub(crate) enum ImplementationName {
    Cpython,
}

#[derive(Debug, PartialEq)]
pub enum Os {
    Darwin,
    Linux,
    Shared,
    Windows,
}

#[derive(Debug, PartialEq)]
pub struct Platform {
    os: Os,
    arch: Arch,
    libc: Libc,
}

include!("python_versions.inc");

impl PythonDownloadMetadata {
    /// Return the [`PythonDownloadMetadata`] corresponding to the key, if it exists.
    pub fn from_key(key: &str) -> Option<&PythonDownloadMetadata> {
        for (ikey, value) in PYTHON_DOWNLOADS {
            if *ikey == key {
                return Some(value);
            }
        }
        None
    }
}

impl Platform {
    pub fn new(os: Os, arch: Arch, libc: Libc) -> Self {
        Self { os, arch, libc }
    }
    pub fn from_env() -> Result<Self, DownloadMetadataError> {
        Ok(Self::new(
            Os::from_env()?,
            Arch::from_env()?,
            Libc::from_env()?,
        ))
    }
}

impl Arch {
    fn from_env() -> Result<Self, DownloadMetadataError> {
        match std::env::consts::ARCH {
            "arm64" => Ok(Arch::Arm64),
            "i686" => Ok(Arch::I686),
            "ppc64le" => Ok(Arch::Ppc64Le),
            "s390x" => Ok(Arch::S390X),
            "x86_64" => Ok(Arch::X86_64),
            arch => Err(DownloadMetadataError::ArchNotSupported(arch)),
        }
    }
}

impl Os {
    fn from_env() -> Result<Self, DownloadMetadataError> {
        match std::env::consts::OS {
            "linux" => Ok(Os::Linux),
            "windows" => Ok(Os::Windows),
            "macos" => Ok(Os::Darwin),
            os => Err(DownloadMetadataError::OsNotSupported(os)),
        }
    }
}

impl Libc {
    fn from_env() -> Result<Self, DownloadMetadataError> {
        // TODO(zanieb): Perform this lookup
        match std::env::consts::OS {
            "linux" | "macos" => Ok(Libc::Gnu),
            "windows" => Ok(Libc::None),
            _ => Err(DownloadMetadataError::LibcNotDetected()),
        }
    }
}
