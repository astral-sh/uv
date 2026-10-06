//! Install-name editing for thin 64-bit macOS Mach-O dylibs.

use std::num::TryFromIntError;

mod bytes;
mod format;
mod macho;
mod names;

pub use names::InstallName;

/// An error reading or editing a Mach-O image.
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
    /// Inspect the edited image without changing it.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Extract the image, discarding its editing state.
    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}
