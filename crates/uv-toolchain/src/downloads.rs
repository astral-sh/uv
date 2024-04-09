use std::str::FromStr;

use thiserror::Error;
use uv_interpreter::PythonVersion;

#[derive(Error, Debug)]
pub enum DownloadMetadataError {
    #[error("operating system not supported: {0}")]
    OsNotSupported(&'static str),
    #[error("architecture not supported: {0}")]
    ArchNotSupported(&'static str),
    #[error("libc type could not be detected")]
    LibcNotDetected(),
    #[error("invalid python version: {0}")]
    InvalidPythonVersion(String),
}

#[derive(Debug, PartialEq)]
pub struct PythonDownloadMetadata {
    implementation: ImplementationName,
    arch: Arch,
    os: Os,
    libc: Libc,
    major: u8,
    minor: u8,
    patch: u8,
    url: &'static str,
    sha256: Option<&'static str>,
}

#[derive(Debug)]
pub struct PythonDownloadRequest {
    version: Option<PythonVersion>,
    implementation: Option<ImplementationName>,
    arch: Option<Arch>,
    os: Option<Os>,
    libc: Option<Libc>,
}

impl PythonDownloadRequest {
    pub fn new(
        version: Option<PythonVersion>,
        implementation: Option<ImplementationName>,
        arch: Option<Arch>,
        os: Option<Os>,
        libc: Option<Libc>,
    ) -> Self {
        Self {
            version,
            implementation,
            arch,
            os,
            libc,
        }
    }

    #[must_use]
    pub fn with_implementation(mut self, implementation: ImplementationName) -> Self {
        self.implementation = Some(implementation);
        self
    }

    #[must_use]
    pub fn with_arch(mut self, arch: Arch) -> Self {
        self.arch = Some(arch);
        self
    }

    #[must_use]
    pub fn with_os(mut self, os: Os) -> Self {
        self.os = Some(os);
        self
    }

    #[must_use]
    pub fn with_libc(mut self, libc: Libc) -> Self {
        self.libc = Some(libc);
        self
    }

    #[must_use]
    pub fn fill(mut self) -> Result<Self, DownloadMetadataError> {
        if self.implementation.is_none() {
            self.implementation = Some(ImplementationName::Cpython)
        }
        if self.arch.is_none() {
            self.arch = Some(Arch::from_env()?)
        }
        if self.os.is_none() {
            self.os = Some(Os::from_env()?)
        }
        if self.libc.is_none() {
            self.libc = Some(Libc::from_env()?)
        }
        Ok(self)
    }
}

impl FromStr for PythonDownloadRequest {
    type Err = DownloadMetadataError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // TOOD(zanieb): Implement parsing of additional request parts
        let version = PythonVersion::from_str(s)
            .map_err(|err| DownloadMetadataError::InvalidPythonVersion(err))?;
        Ok(Self::new(Some(version), None, None, None, None))
    }
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
pub enum ImplementationName {
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

    pub fn from_request(
        request: &PythonDownloadRequest,
    ) -> Option<&'static PythonDownloadMetadata> {
        for (_, download) in PYTHON_DOWNLOADS {
            if let Some(arch) = &request.arch {
                if download.arch != *arch {
                    continue;
                }
            }
            if let Some(os) = &request.os {
                if download.os != *os {
                    continue;
                }
            }
            if let Some(implementation) = &request.implementation {
                if download.implementation != *implementation {
                    continue;
                }
            }
            if let Some(version) = &request.version {
                if download.major != version.major() {
                    continue;
                }
                if download.minor != version.minor() {
                    continue;
                }
                if let Some(patch) = version.patch() {
                    if download.patch != patch {
                        continue;
                    }
                }
            }
            return Some(download);
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
            "arm64" | "aarch64" => Ok(Arch::Arm64),
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
