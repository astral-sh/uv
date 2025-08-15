use owo_colors::OwoColorize;
use thiserror::Error;

pub use crate::discovery::{CudaInstallationKey, CudaRequest, CudaSource, find_cuda_installations};
pub use crate::downloads::{CudaDownloadRequest, CudaPlatformRequest, SimpleProgressReporter};
pub use crate::installation::CudaInstallation;
pub use crate::managed::{
    Error as ManagedError, ManagedCudaInstallation, ManagedCudaInstallations,
};
pub use crate::version::CudaVersion;

mod discovery;
mod downloads;
mod installation;
mod managed;
mod version;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Managed(#[from] managed::Error),

    #[error(transparent)]
    Download(#[from] downloads::Error),

    #[error("{}{}", .0, if let Some(hint) = .1 { format!("\n\n{}{} {hint}", "hint".bold().cyan(), ":".bold()) } else { String::new() })]
    MissingCuda(CudaNotFound, Option<String>),
}

#[derive(Clone, Debug, Error)]
pub struct CudaNotFound {
    pub request: CudaRequest,
}

impl std::fmt::Display for CudaNotFound {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "No CUDA installation found for {}", self.request)
    }
}

impl From<CudaNotFound> for Error {
    fn from(err: CudaNotFound) -> Self {
        Error::MissingCuda(err, None)
    }
}
