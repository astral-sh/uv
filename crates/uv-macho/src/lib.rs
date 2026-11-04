//! Install-name editing and ad-hoc signing for thin 64-bit macOS Mach-O dylibs.

use std::ffi::CStr;
use std::num::TryFromIntError;

mod bytes;
mod macho;
mod regions;
mod signature;

/// An error reading, editing, or signing a Mach-O image.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("Malformed Mach-O: {0}")]
    Malformed(&'static str),
    #[error("Unsupported Mach-O: {0}")]
    Unsupported(&'static str),
    #[error("Unsupported Mach-O {field}: {value:#x}")]
    UnsupportedValue { field: &'static str, value: u64 },
    #[error("Unsupported Mach-O signature, expected a single ad-hoc SHA-256 CodeDirectory")]
    UnsupportedSignature,
    #[error("Not enough Mach-O header padding for the install name and code signature")]
    InsufficientHeaderPadding,
    #[error("The install name must be nonempty and contain no NUL bytes")]
    InvalidName,
    #[error("The signing identifier must be nonempty and contain no NUL bytes")]
    InvalidIdentifier,
    #[error("Mach-O image exceeds the supported size")]
    TooLarge,
    #[error("Mach-O integer exceeds the supported size")]
    IntegerConversion(#[from] TryFromIntError),
}

/// A dylib with a completed ad-hoc signature covering its bytes.
#[derive(Debug, PartialEq, Eq)]
pub struct SignedDylib {
    bytes: Vec<u8>,
}

impl SignedDylib {
    /// Borrow the signed image without invalidating its signature.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// Replace a dylib's install name and generate an ad-hoc signature.
///
/// Both operations use the original image's validated layout. The input is never modified,
/// and section data stays at its original file offsets. The editor accepts unsigned images and
/// bare ad-hoc SHA-256 signatures. The new signature uses the existing signing identifier or
/// `identifier` for an unsigned image.
pub fn set_install_name(
    image: &[u8],
    name: &CStr,
    identifier: &CStr,
) -> Result<SignedDylib, Error> {
    if name.is_empty() {
        return Err(Error::InvalidName);
    }
    if identifier.is_empty() {
        return Err(Error::InvalidIdentifier);
    }

    Ok(SignedDylib {
        bytes: macho::Layout::parse(image)?.set_install_name(name, identifier)?,
    })
}
