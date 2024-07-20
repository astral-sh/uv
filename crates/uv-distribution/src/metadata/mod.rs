use std::collections::BTreeMap;
use std::path::Path;

use thiserror::Error;

use crate::metadata::lowering::LoweringError;
pub use crate::metadata::requires_dist::{RequiresDist, DEV_DEPENDENCIES};
use pep440_rs::{Version, VersionSpecifiers};
use pypi_types::{HashDigest, Metadata23};
use uv_configuration::PreviewMode;
use uv_normalize::{ExtraName, GroupName, PackageName};
use uv_workspace::WorkspaceError;

mod lowering;
mod requires_dist;

#[derive(Debug, Error)]
pub enum MetadataError {
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
    pub dev_dependencies: BTreeMap<GroupName, Vec<pypi_types::Requirement>>,
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
            dev_dependencies: BTreeMap::default(),
        }
    }

    /// Lower by considering `tool.uv` in `pyproject.toml` if present, used for Git and directory
    /// dependencies.
    pub async fn from_workspace(
        metadata: Metadata23,
        install_path: &Path,
        lock_path: &Path,
        preview_mode: PreviewMode,
    ) -> Result<Self, MetadataError> {
        // Lower the requirements.
        let RequiresDist {
            name,
            requires_dist,
            provides_extras,
            dev_dependencies,
        } = RequiresDist::from_project_maybe_workspace(
            pypi_types::RequiresDist {
                name: metadata.name,
                requires_dist: metadata.requires_dist,
                provides_extras: metadata.provides_extras,
            },
            install_path,
            lock_path,
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
    pub fn from_metadata23(metadata: Metadata23) -> Self {
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
