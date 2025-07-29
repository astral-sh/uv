//! Platform detection for operating system, architecture, and libc.

use thiserror::Error;

pub use crate::arch::{Arch, ArchVariant};
pub use crate::libc::{Libc, LibcDetectionError, LibcVersion};
pub use crate::os::Os;

mod arch;
mod cpuinfo;
mod libc;
mod os;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Unknown operating system: {0}")]
    UnknownOs(String),
    #[error("Unknown architecture: {0}")]
    UnknownArch(String),
    #[error("Unknown libc environment: {0}")]
    UnknownLibc(String),
    #[error("Unsupported variant `{0}` for architecture `{1}`")]
    UnsupportedVariant(String, String),
    #[error(transparent)]
    LibcDetectionError(#[from] crate::libc::LibcDetectionError),
}
