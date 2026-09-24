use std::collections::BTreeMap;

use uv_normalize::{ExtraName, GroupName, PackageName};
use uv_pep440::{Version, VersionSpecifiers};
use uv_pypi_types::{HashDigests, ResolutionMetadata};

use crate::Requirement;

#[derive(Debug, Clone)]
pub struct Metadata {
    // Mandatory fields
    pub name: PackageName,
    pub version: Version,
    // Optional fields
    pub requires_dist: Box<[Requirement]>,
    pub requires_python: Option<VersionSpecifiers>,
    pub provides_extra: Box<[ExtraName]>,
    pub dependency_groups: BTreeMap<GroupName, Box<[Requirement]>>,
    pub dynamic: bool,
}

impl Metadata {
    /// Lower without considering `tool.uv` in `pyproject.toml`, used for index and other archive
    /// dependencies.
    pub fn from_metadata23(metadata: ResolutionMetadata) -> Self {
        // This route handles package metadata rather than explicit user input.
        // Write local dependency paths relative to the lockfile.
        Self::from_resolution_metadata(metadata).with_force_relative(true)
    }

    /// Lower metadata selected from `tool.uv.dependency-metadata`.
    pub fn from_dependency_metadata(metadata: ResolutionMetadata) -> Self {
        // Respect the relative/absolute path preference in user-provided metadata overrides.
        Self::from_resolution_metadata(metadata)
    }

    /// Lower package metadata without selecting an output path policy.
    fn from_resolution_metadata(metadata: ResolutionMetadata) -> Self {
        Self {
            name: metadata.name,
            version: metadata.version,
            requires_dist: Box::into_iter(metadata.requires_dist)
                .map(Requirement::from)
                .collect(),
            requires_python: metadata.requires_python,
            provides_extra: metadata.provides_extra,
            dependency_groups: BTreeMap::default(),
            dynamic: metadata.dynamic,
        }
    }

    /// Set whether local dependency sources should be represented by relative paths.
    ///
    /// Disabling this restores each URL's original path spelling preference.
    #[must_use]
    pub fn with_force_relative(mut self, force_relative: bool) -> Self {
        for requirement in self.requires_dist.iter_mut().chain(
            self.dependency_groups
                .values_mut()
                .flat_map(|requirements| requirements.iter_mut()),
        ) {
            requirement.set_force_relative(force_relative);
        }

        self
    }
}

/// The metadata associated with an archive.
#[derive(Debug, Clone)]
pub struct ArchiveMetadata {
    /// The [`Metadata`] for the underlying distribution.
    pub metadata: Metadata,
    /// Hashes computed from the source or built archive.
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
}

impl From<Metadata> for ArchiveMetadata {
    fn from(metadata: Metadata) -> Self {
        Self {
            metadata,
            hashes: HashDigests::empty(),
        }
    }
}
