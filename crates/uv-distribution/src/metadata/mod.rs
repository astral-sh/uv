use std::collections::BTreeMap;
use std::path::Path;

use thiserror::Error;

use uv_configuration::SourceStrategy;
use uv_distribution_types::{GitSourceUrl, IndexLocations};
use uv_normalize::{ExtraName, GroupName, PackageName};
use uv_pep440::{Version, VersionSpecifiers};
use uv_pypi_types::{HashDigests, ResolutionMetadata};
use uv_workspace::dependency_groups::DependencyGroupError;
use uv_workspace::{WorkspaceCache, WorkspaceError};

pub use crate::metadata::build_requires::BuildRequires;
pub use crate::metadata::lowering::LoweredRequirement;
pub use crate::metadata::lowering::LoweringError;
pub use crate::metadata::requires_dist::{FlatRequiresDist, RequiresDist};

mod build_requires;
mod lowering;
mod requires_dist;

#[derive(Debug, Error)]
pub enum MetadataError {
    #[error(transparent)]
    Workspace(#[from] WorkspaceError),
    #[error(transparent)]
    DependencyGroup(#[from] DependencyGroupError),
    #[error("Failed to parse entry: `{0}`")]
    LoweringError(PackageName, #[source] Box<LoweringError>),
    #[error("Failed to parse entry in group `{0}`: `{1}`")]
    GroupLoweringError(GroupName, PackageName, #[source] Box<LoweringError>),
    #[error("Source entry for `{0}` only applies to extra `{1}`, but the `{1}` extra does not exist. When an extra is present on a source (e.g., `extra = \"{1}\"`), the relevant package must be included in the `project.optional-dependencies` section for that extra (e.g., `project.optional-dependencies = {{ \"{1}\" = [\"{0}\"] }}`).")]
    MissingSourceExtra(PackageName, ExtraName),
    #[error("Source entry for `{0}` only applies to extra `{1}`, but `{0}` was not found under the `project.optional-dependencies` section for that extra. When an extra is present on a source (e.g., `extra = \"{1}\"`), the relevant package must be included in the `project.optional-dependencies` section for that extra (e.g., `project.optional-dependencies = {{ \"{1}\" = [\"{0}\"] }}`).")]
    IncompleteSourceExtra(PackageName, ExtraName),
    #[error("Source entry for `{0}` only applies to dependency group `{1}`, but the `{1}` group does not exist. When a group is present on a source (e.g., `group = \"{1}\"`), the relevant package must be included in the `dependency-groups` section for that extra (e.g., `dependency-groups = {{ \"{1}\" = [\"{0}\"] }}`).")]
    MissingSourceGroup(PackageName, GroupName),
    #[error("Source entry for `{0}` only applies to dependency group `{1}`, but `{0}` was not found under the `dependency-groups` section for that group. When a group is present on a source (e.g., `group = \"{1}\"`), the relevant package must be included in the `dependency-groups` section for that extra (e.g., `dependency-groups = {{ \"{1}\" = [\"{0}\"] }}`).")]
    IncompleteSourceGroup(PackageName, GroupName),
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
    pub dependency_groups: BTreeMap<GroupName, Vec<uv_pypi_types::Requirement>>,
    pub dynamic: bool,
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
            dependency_groups: BTreeMap::default(),
            dynamic: metadata.dynamic,
        }
    }

    /// Lower by considering `tool.uv` in `pyproject.toml` if present, used for Git and directory
    /// dependencies.
    pub async fn from_workspace(
        metadata: ResolutionMetadata,
        install_path: &Path,
        git_source: Option<&GitWorkspaceMember<'_>>,
        locations: &IndexLocations,
        sources: SourceStrategy,
        cache: &WorkspaceCache,
    ) -> Result<Self, MetadataError> {
        // Lower the requirements.
        let requires_dist = uv_pypi_types::RequiresDist {
            name: metadata.name,
            requires_dist: metadata.requires_dist,
            provides_extras: metadata.provides_extras,
            dynamic: metadata.dynamic,
        };
        let RequiresDist {
            name,
            requires_dist,
            provides_extras,
            dependency_groups,
            dynamic,
        } = RequiresDist::from_project_maybe_workspace(
            requires_dist,
            install_path,
            git_source,
            locations,
            sources,
            cache,
        )
        .await?;

        // Combine with the remaining metadata.
        Ok(Self {
            name,
            version: metadata.version,
            requires_dist,
            requires_python: metadata.requires_python,
            provides_extras,
            dependency_groups,
            dynamic,
        })
    }
}

/// The metadata associated with an archive.
#[derive(Debug, Clone)]
pub struct ArchiveMetadata {
    /// The [`Metadata`] for the underlying distribution.
    pub metadata: Metadata,
    /// The hashes of the source or built archive.
    pub hashes: HashDigests,
}

impl ArchiveMetadata {
    /// Lower without considering `tool.uv` in `pyproject.toml`, used for index and other archive
    /// dependencies.
    pub fn from_metadata23(metadata: ResolutionMetadata) -> Self {
        Self {
            metadata: Metadata::from_metadata23(metadata),
            hashes: HashDigests::empty(),
        }
    }

    /// Create an [`ArchiveMetadata`] with the given metadata and hashes.
    pub fn with_hashes(metadata: Metadata, hashes: HashDigests) -> Self {
        Self { metadata, hashes }
    }
}

impl From<Metadata> for ArchiveMetadata {
    fn from(metadata: Metadata) -> Self {
        Self {
            metadata,
            hashes: HashDigests::empty(),
        }
    }
}

/// A workspace member from a checked-out Git repo.
#[derive(Debug, Clone)]
pub struct GitWorkspaceMember<'a> {
    /// The root of the checkout, which may be the root of the workspace or may be above the
    /// workspace root.
    pub fetch_root: &'a Path,
    pub git_source: &'a GitSourceUrl<'a>,
}
