//! Editing support for thin 64-bit macOS Mach-O dylibs.

#[cfg(test)]
mod bytes;
#[cfg(test)]
mod format;
#[cfg(test)]
mod macho;

/// An error validating a Mach-O image.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Malformed Mach-O: {0}")]
    Malformed(&'static str),
    #[error("Unsupported Mach-O: {0}")]
    Unsupported(&'static str),
    #[error("Mach-O image exceeds the supported size")]
    TooLarge,
}
