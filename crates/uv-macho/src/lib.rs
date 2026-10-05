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

/// Replace a dylib's install name using existing header padding.
///
/// The input is never modified, and section data is never relocated. [`InstallName`]
/// accepts non-UTF-8 Unix paths. The returned [`EditedDylib`] must be re-signed
/// before loading it on macOS.
pub fn replace_install_name(image: &[u8], name: InstallName<'_>) -> Result<EditedDylib, Error> {
    Ok(EditedDylib {
        bytes: macho::Layout::parse(image)?.replace_install_name(name)?,
    })
}

/// An edited dylib whose code signature must be regenerated before loading it.
#[derive(Debug, PartialEq, Eq)]
pub struct EditedDylib {
    bytes: Vec<u8>,
}

impl EditedDylib {
    /// Consume the edited image and regenerate its ad-hoc code signature.
    pub fn adhoc_sign(self, identifier: &SigningIdentifier) -> Result<SignedDylib, Error> {
        adhoc_sign(&self.bytes, identifier)
    }

    /// Inspect the edited image without changing it.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Extract the image, discarding its editing state.
    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
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

    /// Extract the image, discarding its signing state.
    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

/// Generate an ad-hoc signature, retaining supported signing metadata.
///
/// Uses `identifier` when the image has no signing identifier. Existing requirements,
/// entitlements, and runtime metadata are retained; certificate identity is removed.
/// Unsupported signing metadata produces an error. The input is never modified.
pub fn adhoc_sign(image: &[u8], identifier: &SigningIdentifier) -> Result<SignedDylib, Error> {
    Ok(SignedDylib {
        bytes: macho::Layout::parse(image)?.adhoc_sign(identifier)?,
    })
}

/// Replace a dylib's install name and generate an ad-hoc signature.
///
/// The intermediate [`EditedDylib`] is consumed by signing. The input is never modified,
/// including when editing or signing fails.
pub fn set_install_name(
    image: &[u8],
    name: &InstallName,
    identifier: &SigningIdentifier,
) -> Result<SignedDylib, Error> {
    replace_install_name(image, name)?.adhoc_sign(identifier)
}
