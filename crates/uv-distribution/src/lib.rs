pub use archive::Archive;
pub use distribution_database::{DistributionDatabase, HttpArchivePointer, LocalArchivePointer};
pub use download::LocalWheel;
pub use error::Error;
pub use git::{is_same_reference, to_precise};
pub use index::{BuiltWheelIndex, RegistryWheelIndex};
use pypi_types::{HashDigest, Metadata23};
pub use reporter::Reporter;
pub use source::SourceDistributionBuilder;

mod archive;
mod distribution_database;
mod download;
mod error;
mod git;
mod index;
mod locks;
mod reporter;
mod source;

/// The metadata associated with an archive.
#[derive(Debug, Clone)]
pub struct ArchiveMetadata {
    /// The [`Metadata23`] for the underlying distribution.
    pub metadata: Metadata23,
    /// The hashes of the source or built archive.
    pub hashes: Vec<HashDigest>,
}

impl From<Metadata23> for ArchiveMetadata {
    fn from(metadata: Metadata23) -> Self {
        Self {
            metadata,
            hashes: vec![],
        }
    }
}
