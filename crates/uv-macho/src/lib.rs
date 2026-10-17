//! Install-name editing and ad-hoc signing for thin 64-bit macOS Mach-O dylibs.

use std::num::TryFromIntError;

mod bytes;
mod format;
mod macho;
mod names;
mod regions;
mod signature;

pub use names::{InstallName, SigningIdentifier};

/// An error reading, editing, or signing a Mach-O image.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("Malformed Mach-O: {0}")]
    Malformed(&'static str),
    #[error("Unsupported Mach-O: {0}")]
    Unsupported(&'static str),
    #[error("Unsupported Mach-O {field}: {value:#x}")]
    UnsupportedValue { field: &'static str, value: u64 },
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
/// and section data is never relocated. Existing requirements, entitlements, and runtime metadata
/// are retained; certificate identity is removed. `identifier` is used when the image has no
/// signing identifier. Unsupported signing metadata produces an error.
pub fn set_install_name(
    image: &[u8],
    name: InstallName<'_>,
    identifier: SigningIdentifier<'_>,
) -> Result<SignedDylib, Error> {
    Ok(SignedDylib {
        bytes: macho::Layout::parse(image)?.set_install_name(name, identifier)?,
    })
}
