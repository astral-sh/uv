use std::collections::BTreeMap;
use std::path::Path;

use thiserror::Error;

use pep440_rs::{Version, VersionSpecifiers};
use pypi_types::{HashDigest, Metadata23};
use uv_configuration::PreviewMode;
use uv_normalize::{ExtraName, PackageName};

use crate::requirement_lowering::{lower_requirement, LoweringError};
use crate::{ProjectWorkspace, WorkspaceError};

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
        // TODO(konsti): Limit discovery for Git checkouts to Git root.
        // TODO(konsti): Cache workspace discovery.
        let Some(project_workspace) =
            ProjectWorkspace::from_maybe_project_root(project_root, None).await?
        else {
            return Ok(Self::from_metadata23(metadata));
        };

        Self::from_project_workspace(metadata, &project_workspace, preview_mode)
    }

    pub fn from_project_workspace(
        metadata: Metadata23,
        project_workspace: &ProjectWorkspace,
        preview_mode: PreviewMode,
    ) -> Result<Metadata, MetadataLoweringError> {
        let empty = BTreeMap::default();
        let sources = project_workspace
            .current_project()
            .pyproject_toml()
            .tool
            .as_ref()
            .and_then(|tool| tool.uv.as_ref())
            .and_then(|uv| uv.sources.as_ref())
            .unwrap_or(&empty);

        let requires_dist = metadata
            .requires_dist
            .into_iter()
            .map(|requirement| {
                let requirement_name = requirement.name.clone();
                lower_requirement(
                    requirement,
                    &metadata.name,
                    project_workspace.project_root(),
                    sources,
                    project_workspace.workspace(),
                    preview_mode,
                )
                .map_err(|err| MetadataLoweringError::LoweringError(requirement_name.clone(), err))
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
