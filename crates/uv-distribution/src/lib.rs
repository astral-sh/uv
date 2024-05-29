pub use archive::Archive;
pub use distribution_database::{DistributionDatabase, HttpArchivePointer, LocalArchivePointer};
pub use download::LocalWheel;
pub use error::Error;
pub use git::{git_url_to_precise, is_same_reference};
pub use index::{BuiltWheelIndex, RegistryWheelIndex};
use pypi_types::{HashDigest, Metadata23};
pub use pyproject::*;
pub use reporter::Reporter;
pub use requirement_lowering::{lower_requirement, lower_requirements, LoweringError};
pub use workspace::{ProjectWorkspace, Workspace, WorkspaceError, WorkspaceMember};

mod archive;
mod distribution_database;
mod download;
mod error;
mod git;
mod index;
mod locks;
pub mod pyproject;
mod reporter;
mod requirement_lowering;
mod source;
mod workspace;

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
