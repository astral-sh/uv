//! Binary download and installation utilities for uv.
//!
//! This crate provides functionality for downloading and caching binary tools
//! from various sources (GitHub releases, etc.) for use by uv.

pub use download::BinaryDownloader;
pub use error::Error;
pub use ruff::RuffDownloader;

pub mod download;
pub mod error;
pub mod ruff;

/// Result type for binary installation operations.
pub type Result<T> = std::result::Result<T, Error>;