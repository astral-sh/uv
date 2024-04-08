use std::fmt::Display;
use std::io;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use thiserror::Error;
use uv_client::BetterReqwestError;
use uv_interpreter::PythonVersion;

use futures::TryStreamExt;

use tokio_util::compat::FuturesAsyncReadCompatExt;
use tracing::debug;
use url::Url;
use uv_fs::Simplified;

#[derive(Error, Debug)]
pub enum Error {
    #[error("operating system not supported: {0}")]
    OsNotSupported(&'static str),
    #[error("architecture not supported: {0}")]
    ArchNotSupported(&'static str),
    #[error("libc type could not be detected")]
    LibcNotDetected(),
    #[error("invalid python version: {0}")]
    InvalidPythonVersion(String),
    #[error("download failed")]
    NetworkError(#[from] BetterReqwestError),
    #[error("download failed")]
    NetworkMiddlewareError(#[source] anyhow::Error),
    #[error(transparent)]
    ExtractError(#[from] uv_extract::Error),
    #[error("invalid download url")]
    InvalidUrl(#[from] url::ParseError),
    #[error("failed to create download directory")]
    DownloadDirError(#[source] io::Error),
    #[error("failed to copy to: {0}", to.user_display())]
    CopyError {
        to: PathBuf,
        #[source]
        err: io::Error,
    },
}

#[derive(Debug, PartialEq)]
pub struct PythonDownload {
    key: &'static str,
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

    pub fn fill(mut self) -> Result<Self, Error> {
        if self.implementation.is_none() {
            self.implementation = Some(ImplementationName::Cpython);
        }
        if self.arch.is_none() {
            self.arch = Some(Arch::from_env()?);
        }
        if self.os.is_none() {
            self.os = Some(Os::from_env()?);
        }
        if self.libc.is_none() {
            self.libc = Some(Libc::from_env()?);
        }
        Ok(self)
    }
}

impl FromStr for PythonDownloadRequest {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // TOOD(zanieb): Implement parsing of additional request parts
        let version = PythonVersion::from_str(s).map_err(Error::InvalidPythonVersion)?;
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

impl PythonDownload {
    /// Return the [`PythonDownload`] corresponding to the key, if it exists.
    pub fn from_key(key: &str) -> Option<&PythonDownload> {
        PYTHON_DOWNLOADS.iter().find(|&value| value.key == key)
    }

    pub fn from_request(request: &PythonDownloadRequest) -> Option<&'static PythonDownload> {
        for download in PYTHON_DOWNLOADS {
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

    pub fn url(&self) -> &str {
        self.url
    }

    pub fn sha256(&self) -> Option<&str> {
        self.sha256
    }

    /// Download and extract
    pub async fn fetch(
        &self,
        client: &uv_client::BaseClient,
        path: &Path,
    ) -> Result<PathBuf, Error> {
        let url = Url::parse(self.url)?;
        let path = path.join(self.key).clone();

        // If it already exists, return it
        if path.is_dir() {
            return Ok(path);
        }

        let filename = url.path_segments().unwrap().last().unwrap();
        let response = client.get(url.clone()).send().await?;

        // Ensure the request was successful.
        response.error_for_status_ref()?;

        // Download and extract into a temporary directory.
        let temp_dir = tempfile::tempdir().map_err(Error::DownloadDirError)?;

        debug!(
            "Downloading {url} to temporary location {}",
            temp_dir.path().display()
        );
        let reader = response
            .bytes_stream()
            .map_err(|err| std::io::Error::new(std::io::ErrorKind::Other, err))
            .into_async_read();

        debug!("Extracting {filename}");
        uv_extract::stream::archive(reader.compat(), filename, temp_dir.path()).await?;

        // Extract the top-level directory.
        let extracted = match uv_extract::strip_component(temp_dir.path()) {
            Ok(top_level) => top_level,
            Err(uv_extract::Error::NonSingularArchive(_)) => temp_dir.into_path(),
            Err(err) => return Err(err.into()),
        };

        // Persist it to the target
        debug!("Moving {} to {}", extracted.display(), &path.user_display());
        fs_err::tokio::rename(extracted, &path)
            .await
            .map_err(|err| Error::CopyError {
                to: path.clone(),
                err,
            })?;

        Ok(path)
    }

    pub fn python_version(&self) -> PythonVersion {
        PythonVersion::from_str(&format!("{}.{}.{}", self.major, self.minor, self.patch))
            .expect("Python downloads should always have valid versions")
    }
}

impl Platform {
    pub fn new(os: Os, arch: Arch, libc: Libc) -> Self {
        Self { os, arch, libc }
    }
    pub fn from_env() -> Result<Self, Error> {
        Ok(Self::new(
            Os::from_env()?,
            Arch::from_env()?,
            Libc::from_env()?,
        ))
    }
}

impl Arch {
    fn from_env() -> Result<Self, Error> {
        match std::env::consts::ARCH {
            "arm64" | "aarch64" => Ok(Arch::Arm64),
            "i686" => Ok(Arch::I686),
            "ppc64le" => Ok(Arch::Ppc64Le),
            "s390x" => Ok(Arch::S390X),
            "x86_64" => Ok(Arch::X86_64),
            arch => Err(Error::ArchNotSupported(arch)),
        }
    }
}

impl Os {
    fn from_env() -> Result<Self, Error> {
        match std::env::consts::OS {
            "linux" => Ok(Os::Linux),
            "windows" => Ok(Os::Windows),
            "macos" => Ok(Os::Darwin),
            os => Err(Error::OsNotSupported(os)),
        }
    }
}

impl Libc {
    fn from_env() -> Result<Self, Error> {
        // TODO(zanieb): Perform this lookup
        match std::env::consts::OS {
            "linux" | "macos" => Ok(Libc::Gnu),
            "windows" => Ok(Libc::None),
            _ => Err(Error::LibcNotDetected()),
        }
    }
}

impl From<reqwest::Error> for Error {
    fn from(error: reqwest::Error) -> Self {
        Self::NetworkError(BetterReqwestError::from(error))
    }
}

impl From<reqwest_middleware::Error> for Error {
    fn from(error: reqwest_middleware::Error) -> Self {
        match error {
            reqwest_middleware::Error::Middleware(error) => Self::NetworkMiddlewareError(error),
            reqwest_middleware::Error::Reqwest(error) => {
                Self::NetworkError(BetterReqwestError::from(error))
            }
        }
    }
}

impl Display for PythonDownload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.key)
    }
}
