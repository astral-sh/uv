use std::path::Path;

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub use archive::Archive;
pub use distribution_database::{DistributionDatabase, HttpArchivePointer, LocalArchivePointer};
pub use download::LocalWheel;
pub use error::Error;
pub use git::{git_url_to_precise, is_same_reference};
pub use index::{BuiltWheelIndex, RegistryWheelIndex};
use pep440_rs::{Version, VersionSpecifiers};
use pypi_types::{HashDigest, Metadata23};
pub use pyproject::*;
pub use reporter::Reporter;
pub use requirement_lowering::{lower_requirement, lower_requirements, LoweringError};
use uv_configuration::PreviewMode;
use uv_normalize::{ExtraName, PackageName};
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

#[derive(Debug, Error)]
pub enum MetadataLoweringError {
    #[error(transparent)]
    Workspace(#[from] WorkspaceError),
    #[error(transparent)]
    Lowering(#[from] LoweringError),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct Metadata23Lowered {
    // Mandatory fields
    pub name: PackageName,
    pub version: Version,
    // Optional fields
    pub requires_dist: Vec<pypi_types::Requirement>,
    pub requires_python: Option<VersionSpecifiers>,
    pub provides_extras: Vec<ExtraName>,
}

impl Metadata23Lowered {
    /// Lower without considering `tool.uv` in `pyproject.toml`, used for index and other archive
    /// dependencies.
    pub fn from_plain(metadata: Metadata23) -> Self {
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
    pub async fn from_tool_uv(
        metadata: Metadata23,
        project_root: &Path,
        preview_mode: PreviewMode,
    ) -> Result<Self, MetadataLoweringError> {
        // TODO(konsti): Limit discovery for Git checkouts to Git root.
        // TODO(konsti): Cache workspace discovery.
        let Some(project_workspace) =
            ProjectWorkspace::from_maybe_project_root(project_root).await?
        else {
            return Ok(Self::from_plain(metadata));
        };

        let sources = project_workspace
            .current_project()
            .pyproject_toml()
            .tool
            .as_ref()
            .and_then(|tool| tool.uv.as_ref())
            .and_then(|uv| uv.sources.clone())
            .unwrap_or_default();
        let requires_dist = metadata
            .requires_dist
            .into_iter()
            .map(|requirement| {
                lower_requirement(
                    requirement,
                    &metadata.name,
                    project_workspace.project_root(),
                    &sources,
                    project_workspace.workspace(),
                    preview_mode,
                )
            })
            .collect::<Result<_, _>>()?;

        Ok(Self {
            name: metadata.name,
            version: metadata.version,
            requires_dist,
            requires_python: metadata.requires_python,
            provides_extras: metadata.provides_extras,
        })
    }
}

/// The metadata associated with an archive.
#[derive(Debug, Clone)]
pub struct ArchiveMetadataLowered {
    /// The [`Metadata23`] for the underlying distribution.
    pub metadata: Metadata23Lowered,
    /// The hashes of the source or built archive.
    pub hashes: Vec<HashDigest>,
}

impl ArchiveMetadataLowered {
    /// Lower without considering `tool.uv` in `pyproject.toml`, used for index and other archive
    /// dependencies.
    pub fn from_plain(metadata: ArchiveMetadata) -> Self {
        Self {
            metadata: Metadata23Lowered::from_plain(metadata.metadata),
            hashes: metadata.hashes,
        }
    }

    /// Lower by considering `tool.uv` in `pyproject.toml` if present, used for git and directory
    /// dependencies.
    // TODO(konsti): Workspace caching
    pub async fn from_tool_uv(
        metadata: ArchiveMetadata,
        project_root: &Path,
        preview_mode: PreviewMode,
    ) -> Result<Self, MetadataLoweringError> {
        let metadata_lowered =
            Metadata23Lowered::from_tool_uv(metadata.metadata, project_root, preview_mode).await?;
        Ok(Self {
            metadata: metadata_lowered,
            hashes: metadata.hashes,
        })
    }
}

impl From<Metadata23Lowered> for ArchiveMetadataLowered {
    fn from(metadata: Metadata23Lowered) -> Self {
        Self {
            metadata,
            hashes: vec![],
        }
    }
}
