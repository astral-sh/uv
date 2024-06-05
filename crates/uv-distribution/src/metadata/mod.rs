use std::path::Path;

use thiserror::Error;

use pep440_rs::{Version, VersionSpecifiers};
use pypi_types::{HashDigest, Metadata23};
pub use requires_dist::RequiresDist;
use uv_configuration::PreviewMode;
use uv_normalize::{ExtraName, PackageName};

use crate::metadata::lowering::LoweringError;
use crate::WorkspaceError;

mod lowering;
mod requires_dist;

#[derive(Debug, Error)]
pub enum MetadataLoweringError {
    #[error(transparent)]
    Workspace(#[from] WorkspaceError),
    #[error("Failed to parse entry for: `{0}`")]
    LoweringError(PackageName, #[source] LoweringError),
}

#[derive(Debug, Clone)]
pub struct Metadata {
    // Mandatory fields
    pub name: PackageName,
    pub version: Version,
    // Optional fields
    pub requires_dist: Vec<pypi_types::Requirement>,
    pub requires_python: Option<VersionSpecifiers>,
    pub provides_extras: Vec<ExtraName>,
}

impl Metadata {
    /// Lower without considering `tool.uv` in `pyproject.toml`, used for index and other archive
    /// dependencies.
    pub fn from_metadata23(metadata: Metadata23) -> Self {
        Self {
            name: metadata.name,
            version: metadata.version,
            requires_dist: metadata
                .requires_dist
                .into_iter()
                .map(pypi_types::Requirement::from)
                .collect(),
            requires_python: metadata.requires_python,
            provides_extras: metadata.provides_extras,
        }
    }

    /// Lower by considering `tool.uv` in `pyproject.toml` if present, used for Git and directory
    /// dependencies.
    pub async fn from_workspace(
        metadata: Metadata23,
        project_root: &Path,
        preview_mode: PreviewMode,
    ) -> Result<Self, MetadataLoweringError> {
        // Lower the requirements.
        let RequiresDist {
            name,
            requires_dist,
            provides_extras,
        } = RequiresDist::from_workspace(
            pypi_types::RequiresDist {
                name: metadata.name,
                requires_dist: metadata.requires_dist,
                provides_extras: metadata.provides_extras,
            },
            project_root,
            preview_mode,
        )
        .await?;

        // Combine with the remaining metadata.
        Ok(Self {
            name,
            version: metadata.version,
            requires_dist,
            requires_python: metadata.requires_python,
            provides_extras,
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
    pub fn from_metadata23(metadata: Metadata23) -> Self {
        Self {
            metadata: Metadata::from_metadata23(metadata),
            hashes: vec![],
        }
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
