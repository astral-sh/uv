use std::collections::BTreeMap;
use std::path::Path;

use thiserror::Error;

use uv_configuration::SourceStrategy;
use uv_normalize::{ExtraName, GroupName, PackageName};
use uv_pep440::{Version, VersionSpecifiers};
use uv_pypi_types::{HashDigest, ResolutionMetadata};
use uv_workspace::WorkspaceError;

pub use crate::metadata::lowering::LoweredRequirement;
use crate::metadata::lowering::LoweringError;
pub use crate::metadata::requires_dist::RequiresDist;

mod lowering;
mod requires_dist;

#[derive(Debug, Error)]
pub enum MetadataError {
    #[error(transparent)]
    Workspace(#[from] WorkspaceError),
    #[error("Failed to parse entry for: `{0}`")]
    LoweringError(PackageName, #[source] LoweringError),
    #[error(transparent)]
    Lower(#[from] LoweringError),
}

#[derive(Debug, Clone)]
pub struct Metadata {
    // Mandatory fields
    pub name: PackageName,
    pub version: Version,
    // Optional fields
    pub requires_dist: Vec<uv_pypi_types::Requirement>,
    pub requires_python: Option<VersionSpecifiers>,
    pub provides_extras: Vec<ExtraName>,
    pub dev_dependencies: BTreeMap<GroupName, Vec<uv_pypi_types::Requirement>>,
}

impl Metadata {
    /// Lower without considering `tool.uv` in `pyproject.toml`, used for index and other archive
    /// dependencies.
    pub fn from_metadata23(metadata: ResolutionMetadata) -> Self {
        Self {
            name: metadata.name,
            version: metadata.version,
            requires_dist: metadata
                .requires_dist
                .into_iter()
                .map(uv_pypi_types::Requirement::from)
                .collect(),
            requires_python: metadata.requires_python,
            provides_extras: metadata.provides_extras,
            dev_dependencies: BTreeMap::default(),
        }
    }

    /// Lower by considering `tool.uv` in `pyproject.toml` if present, used for Git and directory
    /// dependencies.
    pub async fn from_workspace(
        metadata: ResolutionMetadata,
        install_path: &Path,
        sources: SourceStrategy,
    ) -> Result<Self, MetadataError> {
        // Lower the requirements.
        let RequiresDist {
            name,
            requires_dist,
            provides_extras,
            dev_dependencies,
        } = RequiresDist::from_project_maybe_workspace(
            uv_pypi_types::RequiresDist {
                name: metadata.name,
                requires_dist: metadata.requires_dist,
                provides_extras: metadata.provides_extras,
            },
            install_path,
            sources,
        )
        .await?;

        // Combine with the remaining metadata.
        Ok(Self {
            name,
            version: metadata.version,
            requires_dist,
            requires_python: metadata.requires_python,
            provides_extras,
            dev_dependencies,
        })
    }
}

/// The metadata associated with an archive.
#[derive(Debug, Clone)]
pub struct ArchiveMetadata {
    /// The [`Metadata`] for the underlying distribution.
    pub metadata: Metadata,
    /// The hashes of the source or built archive.
    pub hashes: Vec<HashDigest>,
}

impl ArchiveMetadata {
    /// Lower without considering `tool.uv` in `pyproject.toml`, used for index and other archive
    /// dependencies.
    pub fn from_metadata23(metadata: ResolutionMetadata) -> Self {
        Self {
            metadata: Metadata::from_metadata23(metadata),
            hashes: vec![],
        }
    }

    /// Create an [`ArchiveMetadata`] with the given metadata and hashes.
    pub fn with_hashes(metadata: Metadata, hashes: Vec<HashDigest>) -> Self {
        Self { metadata, hashes }
    }
}

impl From<Metadata> for ArchiveMetadata {
    fn from(metadata: Metadata) -> Self {
        Self {
            metadata,
            hashes: vec![],
        }
    }
}
