pub use archive::Archive;
pub use distribution_database::{DistributionDatabase, HttpArchivePointer, LocalArchivePointer};
pub use download::LocalWheel;
pub use error::Error;
pub use git::{git_url_to_precise, is_same_reference};
pub use index::{BuiltWheelIndex, RegistryWheelIndex};
use pypi_types::{HashDigest, Metadata23};
pub use reporter::Reporter;
pub use source::SourceDistributionBuilder;
use uv_types::{BuildContext, SourceBuildTrait};

mod archive;
mod distribution_database;
mod download;
mod error;
mod git;
mod index;
mod locks;
mod reporter;
mod source;

/// Here we have access to both `uv_build` and `BuildContext`, so we can refine the trait with the
/// actual error type.
pub trait BuildContextWithErr: BuildContext<SourceDistBuilder = Self::Builder> {
    type Builder: SourceBuildTrait<Err = uv_build::Error>;
}

impl<U, T> BuildContextWithErr for T
where
    T: BuildContext<SourceDistBuilder = U>,
    U: SourceBuildTrait<Err = uv_build::Error>,
{
    type Builder = <T as BuildContext>::SourceDistBuilder;
}

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
