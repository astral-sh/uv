//! Editing support for thin 64-bit macOS Mach-O dylibs.

use std::num::TryFromIntError;

#[cfg(test)]
mod bytes;
#[cfg(test)]
mod format;
#[cfg(test)]
mod macho;

/// An error validating a Mach-O image.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("Malformed Mach-O: {0}")]
    Malformed(&'static str),
    #[error("Unsupported Mach-O: {0}")]
    Unsupported(&'static str),
    #[error("Unsupported Mach-O {field}: {value:#x}")]
    UnsupportedValue { field: &'static str, value: u64 },
    #[error("Mach-O image exceeds the supported size")]
    TooLarge,
    #[error("Mach-O integer exceeds the supported size")]
    IntegerConversion(#[from] TryFromIntError),
}
